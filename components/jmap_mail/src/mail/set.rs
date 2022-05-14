use crate::mail::import::JMAPMailImport;
use crate::mail::parse::get_message_part;
use crate::mail::{
    HeaderName, Keyword, MailHeaderForm, MailHeaderProperty, MailProperty, MessageField,
};
use jmap::error::set::{SetError, SetErrorType};
use jmap::id::blob::JMAPBlob;
use jmap::id::JMAPIdSerialize;
use jmap::jmap_store::blob::JMAPBlobStore;
use jmap::jmap_store::orm::{JMAPOrm, TinyORM};
use jmap::jmap_store::set::{DefaultUpdateItem, SetObject, SetObjectData, SetObjectHelper};
use jmap::protocol::invocation::Invocation;
use jmap::protocol::json::JSONValue;
use jmap::request::set::SetRequest;
use mail_builder::headers::address::Address;
use mail_builder::headers::content_type::ContentType;
use mail_builder::headers::date::Date;
use mail_builder::headers::message_id::MessageId;
use mail_builder::headers::raw::Raw;
use mail_builder::headers::text::Text;
use mail_builder::headers::url::URL;
use mail_builder::mime::{BodyPart, MimePart};
use mail_builder::MessageBuilder;
use std::collections::{BTreeMap, HashMap, HashSet};
use store::core::collection::Collection;
use store::core::document::Document;
use store::core::error::StoreError;
use store::core::tag::Tag;
use store::core::JMAPIdPrefix;
use store::write::options::{IndexOptions, Options};

use store::blob::BlobId;
use store::chrono::DateTime;

use store::roaring::RoaringBitmap;
use store::{AccountId, DocumentId, JMAPId, JMAPStore, Store};

use super::import::MailImportResult;
use super::MessageData;

#[allow(clippy::large_enum_variant)]
pub enum SetMail {
    Create {
        fields: TinyORM<MessageField>,
        received_at: Option<u64>,
        builder: MessageBuilder,
        body_values: Option<HashMap<String, JSONValue>>,
    },
    Update {
        current_fields: TinyORM<MessageField>,
        fields: TinyORM<MessageField>,
    },
}

impl SetMail {
    fn get_orm(&mut self) -> &mut TinyORM<MessageField> {
        match self {
            SetMail::Create { fields, .. } => fields,
            SetMail::Update { fields, .. } => fields,
        }
    }
}

pub struct SetMailHelper {
    pub mailbox_ids: RoaringBitmap,
}

impl<T> SetObjectData<T> for SetMailHelper
where
    T: for<'x> Store<'x> + 'static,
{
    fn new(store: &JMAPStore<T>, request: &mut SetRequest) -> jmap::Result<Self> {
        Ok(SetMailHelper {
            mailbox_ids: store
                .get_document_ids(request.account_id, Collection::Mailbox)?
                .unwrap_or_default(),
        })
    }

    fn unwrap_invocation(self) -> Option<Invocation> {
        None
    }
}

impl<'y, T> SetObject<'y, T> for SetMail
where
    T: for<'x> Store<'x> + 'static,
{
    type Property = MailProperty;
    type Helper = SetMailHelper;
    type CreateItemResult = MailImportResult;
    type UpdateItemResult = DefaultUpdateItem;

    fn new(
        helper: &mut SetObjectHelper<T, SetMailHelper>,
        fields: &mut HashMap<String, JSONValue>,
        jmap_id: Option<JMAPId>,
    ) -> jmap::error::set::Result<Self> {
        Ok(if let Some(jmap_id) = jmap_id {
            let current_fields = helper
                .store
                .get_orm(helper.account_id, jmap_id.get_document_id())?
                .ok_or_else(|| SetError::new_err(SetErrorType::NotFound))?;
            SetMail::Update {
                fields: TinyORM::track_changes(&current_fields),
                current_fields,
            }
        } else {
            Self::Create {
                fields: TinyORM::new(),
                received_at: None,
                builder: MessageBuilder::new(),
                body_values: fields.remove("bodyValues").and_then(|v| v.unwrap_object()),
            }
        })
    }

    fn set_field(
        &mut self,
        helper: &mut SetObjectHelper<T, SetMailHelper>,
        field: Self::Property,
        value: JSONValue,
    ) -> jmap::error::set::Result<()> {
        match (field, self) {
            (
                field @ MailProperty::MailboxIds,
                SetMail::Create { fields, .. } | SetMail::Update { fields, .. },
            ) => {
                fields.untag_all(&MessageField::Mailbox);

                for (mailbox, value) in value.unwrap_object().ok_or_else(|| {
                    SetError::invalid_property(
                        field.to_string(),
                        "Expected object containing mailboxIds",
                    )
                })? {
                    if let (Some(mailbox_id), Some(set)) =
                        (JMAPId::from_jmap_string(&mailbox), value.to_bool())
                    {
                        if set {
                            let mailbox_id = mailbox_id.get_document_id();
                            if helper.data.mailbox_ids.contains(mailbox_id) {
                                fields.tag(MessageField::Mailbox, Tag::Id(mailbox_id));
                            } else {
                                return Err(SetError::invalid_property(
                                    field.to_string(),
                                    format!("mailboxId {} does not exist.", mailbox),
                                ));
                            }
                        }
                    } else {
                        return Err(SetError::invalid_property(
                            field.to_string(),
                            "Expected boolean value in mailboxIds",
                        ));
                    }
                }
            }
            (
                field @ MailProperty::Keywords,
                SetMail::Create { fields, .. } | SetMail::Update { fields, .. },
            ) => {
                fields.untag_all(&MessageField::Keyword);

                for (keyword, value) in value.unwrap_object().ok_or_else(|| {
                    SetError::invalid_property(
                        field.to_string(),
                        "Expected object containing keywords",
                    )
                })? {
                    if value.to_bool().ok_or_else(|| {
                        SetError::invalid_property(
                            field.to_string(),
                            "Expected boolean value in keywords",
                        )
                    })? {
                        fields.tag(
                            MessageField::Keyword,
                            Keyword::from_jmap(keyword.to_string()),
                        );
                    }
                }
            }
            (MailProperty::ReceivedAt, SetMail::Create { received_at, .. }) => {
                *received_at = value.parse_json_date()?.into();
            }
            (MailProperty::MessageId, SetMail::Create { builder, .. }) => builder.header(
                "Message-ID",
                MessageId::from(value.parse_json_string_list()?),
            ),
            (MailProperty::InReplyTo, SetMail::Create { builder, .. }) => builder.header(
                "In-Reply-To",
                MessageId::from(value.parse_json_string_list()?),
            ),
            (MailProperty::References, SetMail::Create { builder, .. }) => builder.header(
                "References",
                MessageId::from(value.parse_json_string_list()?),
            ),
            (MailProperty::Sender, SetMail::Create { builder, .. }) => {
                builder.header("Sender", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::From, SetMail::Create { builder, .. }) => {
                builder.header("From", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::To, SetMail::Create { builder, .. }) => {
                builder.header("To", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::Cc, SetMail::Create { builder, .. }) => {
                builder.header("Cc", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::Bcc, SetMail::Create { builder, .. }) => {
                builder.header("Bcc", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::ReplyTo, SetMail::Create { builder, .. }) => {
                builder.header("Reply-To", Address::List(value.parse_json_addresses()?))
            }
            (MailProperty::Subject, SetMail::Create { builder, .. }) => {
                builder.header("Subject", Text::new(value.parse_json_string()?));
            }
            (MailProperty::SentAt, SetMail::Create { builder, .. }) => {
                builder.header("Date", Date::new(value.parse_json_date()? as i64))
            }
            (
                field @ MailProperty::TextBody,
                SetMail::Create {
                    builder,
                    body_values,
                    ..
                },
            ) => {
                builder.text_body = value
                    .parse_body_parts(helper, body_values, "text/plain".into(), true)?
                    .pop()
                    .ok_or_else(|| {
                        SetError::invalid_property(
                            field.to_string(),
                            "No text body part found".to_string(),
                        )
                    })?
                    .into();
            }
            (
                field @ MailProperty::HtmlBody,
                SetMail::Create {
                    builder,
                    body_values,
                    ..
                },
            ) => {
                builder.html_body = value
                    .parse_body_parts(helper, body_values, "text/html".into(), true)?
                    .pop()
                    .ok_or_else(|| {
                        SetError::invalid_property(
                            field.to_string(),
                            "No html body part found".to_string(),
                        )
                    })?
                    .into();
            }
            (
                MailProperty::Attachments,
                SetMail::Create {
                    builder,
                    body_values,
                    ..
                },
            ) => {
                builder.attachments = value
                    .parse_body_parts(helper, body_values, None, false)?
                    .into();
            }
            (
                MailProperty::BodyStructure,
                SetMail::Create {
                    builder,
                    body_values,
                    ..
                },
            ) => {
                builder.body = value.parse_body_structure(helper, body_values)?.into();
            }
            (
                MailProperty::Header(MailHeaderProperty { form, header, all }),
                SetMail::Create { builder, .. },
            ) => {
                if !all {
                    value.parse_header(builder, header, form)?;
                } else {
                    for value in value.unwrap_array().ok_or_else(|| {
                        SetError::invalid_property(
                            "header".to_string(),
                            "Expected an array.".to_string(),
                        )
                    })? {
                        value.parse_header(builder, header.clone(), form)?;
                    }
                }
            }

            (field, _) => {
                return Err(SetError::invalid_property(
                    field.to_string(),
                    "Property cannot be set.",
                ));
            }
        }

        Ok(())
    }

    fn patch_field(
        &mut self,
        helper: &mut SetObjectHelper<T, SetMailHelper>,
        field: Self::Property,
        property: String,
        value: JSONValue,
    ) -> jmap::error::set::Result<()> {
        let (property, tag) = match &field {
            MailProperty::MailboxIds => match JMAPId::from_jmap_string(property.as_ref()) {
                Some(mailbox_id) => {
                    let document_id = mailbox_id.get_document_id();
                    if helper.data.mailbox_ids.contains(document_id) {
                        (MessageField::Mailbox, Tag::Id(document_id))
                    } else {
                        return Err(SetError::invalid_property(
                            field.to_string(),
                            format!("mailboxId {} does not exist.", property),
                        ));
                    }
                }
                None => {
                    return Err(SetError::invalid_property(
                        format!("{}/{}", field, property),
                        "Invalid JMAP Id",
                    ));
                }
            },
            MailProperty::Keywords => (MessageField::Keyword, Keyword::from_jmap(property)),
            _ => {
                return Err(SetError::invalid_property(
                    format!("{}/{}", field, property),
                    "Unsupported property.",
                ));
            }
        };

        match value {
            JSONValue::Null | JSONValue::Bool(false) => self.get_orm().untag(&property, &tag),
            JSONValue::Bool(true) => self.get_orm().tag(property, tag),
            _ => {
                return Err(SetError::invalid_property(
                    format!("{}/{}", field, property),
                    "Expected a boolean or null value.",
                ));
            }
        }

        Ok(())
    }

    fn create(
        self,
        helper: &mut SetObjectHelper<T, SetMailHelper>,
        _create_id: &str,
        document: &mut Document,
    ) -> jmap::error::set::Result<Self::CreateItemResult> {
        if let SetMail::Create {
            fields,
            received_at,
            builder,
            ..
        } = self
        {
            if !fields.has_tags(&MessageField::Mailbox) {
                return Err(SetError::new(
                    SetErrorType::InvalidProperties,
                    "Message has to belong to at least one mailbox.",
                ));
            }

            if builder.headers.is_empty()
                && builder.body.is_none()
                && builder.html_body.is_none()
                && builder.text_body.is_none()
                && builder.attachments.is_none()
            {
                return Err(SetError::new(
                    SetErrorType::InvalidProperties,
                    "Message has to have at least one header or body part.",
                ));
            }

            // Store blob
            let mut blob = Vec::with_capacity(1024);
            builder.write_to(&mut blob).map_err(|_| {
                StoreError::SerializeError("Failed to write to memory.".to_string())
            })?;
            let blob_id = helper.store.blob_store(&blob)?;
            let jmap_blob_id: JMAPBlob = (&blob_id).into();

            // Add mailbox tags
            for mailbox_tag in fields.get_tags(&MessageField::Mailbox).unwrap() {
                helper
                    .changes
                    .log_child_update(Collection::Mailbox, mailbox_tag.as_id() as JMAPId);
            }

            // Parse message
            // TODO: write parsed message directly to store, avoid parsing it again.
            let size = blob.len();
            helper
                .store
                .mail_parse(document, blob_id, &blob, received_at)?;
            fields.insert(document)?;

            // Lock collection
            helper.lock(Collection::Mail);

            // Obtain thread Id
            let thread_id = helper
                .store
                .mail_set_thread(&mut helper.changes, document)?;

            Ok(MailImportResult {
                id: JMAPId::from_parts(thread_id, document.document_id),
                blob_id: jmap_blob_id,
                thread_id,
                size,
            })
        } else {
            unreachable!()
        }
    }

    fn update(
        self,
        helper: &mut SetObjectHelper<T, SetMailHelper>,
        document: &mut Document,
    ) -> jmap::error::set::Result<Option<Self::UpdateItemResult>> {
        if let SetMail::Update {
            fields,
            current_fields,
        } = self
        {
            if !fields.has_tags(&MessageField::Mailbox) {
                return Err(SetError::new(
                    SetErrorType::InvalidProperties,
                    "Message has to belong to at least one mailbox.",
                ));
            }

            // Set all current mailboxes as changed if the Seen tag changed
            let mut changed_mailboxes = HashSet::new();
            if current_fields
                .get_changed_tags(&fields, &MessageField::Keyword)
                .iter()
                .any(|keyword| matches!(keyword, Tag::Static(k_id) if k_id == &Keyword::SEEN))
            {
                for mailbox_tag in fields.get_tags(&MessageField::Mailbox).unwrap() {
                    changed_mailboxes.insert(mailbox_tag.as_id());
                }
            }

            // Add all new or removed mailboxes
            for changed_mailbox_tag in
                current_fields.get_changed_tags(&fields, &MessageField::Mailbox)
            {
                changed_mailboxes.insert(changed_mailbox_tag.as_id());
            }

            // Log mailbox changes
            if !changed_mailboxes.is_empty() {
                for changed_mailbox_id in changed_mailboxes {
                    helper
                        .changes
                        .log_child_update(Collection::Mailbox, changed_mailbox_id);
                }
            }

            // Merge changes
            current_fields.merge_validate(document, fields)?;

            if !document.is_empty() {
                Ok(Some(DefaultUpdateItem::default()))
            } else {
                Ok(None)
            }
        } else {
            unreachable!()
        }
    }

    fn validate_delete(
        _helper: &mut SetObjectHelper<T, Self::Helper>,
        _jmap_id: JMAPId,
    ) -> jmap::error::set::Result<()> {
        Ok(())
    }

    fn delete(
        store: &JMAPStore<T>,
        account_id: AccountId,
        document: &mut Document,
    ) -> store::Result<()> {
        let document_id = document.document_id;
        let metadata_blob_id = if let Some(metadata_blob_id) = store.get_document_value::<BlobId>(
            account_id,
            Collection::Mail,
            document_id,
            MessageField::Metadata.into(),
        )? {
            metadata_blob_id
        } else {
            return Ok(());
        };

        // Remove index entries
        MessageData::from_metadata(
            &store
                .blob_get(&metadata_blob_id)?
                .ok_or(StoreError::DataCorruption)?,
        )
        .ok_or(StoreError::DataCorruption)?
        .build_index(document, false)?;

        // Remove thread related data
        let thread_id = store
            .get_document_value::<DocumentId>(
                account_id,
                Collection::Mail,
                document_id,
                MessageField::ThreadId.into(),
            )?
            .ok_or(StoreError::DataCorruption)?;
        document.tag(
            MessageField::ThreadId,
            Tag::Id(thread_id),
            IndexOptions::new().clear(),
        );
        document.number(
            MessageField::ThreadId,
            thread_id,
            IndexOptions::new().store().clear(),
        );

        // Unlink metadata
        document.blob(metadata_blob_id, IndexOptions::new().clear());
        document.binary(
            MessageField::Metadata,
            Vec::with_capacity(0),
            IndexOptions::new().clear(),
        );

        // Delete ORM
        let fields = store
            .get_orm::<MessageField>(account_id, document_id)?
            .ok_or(StoreError::DataCorruption)?;
        fields.delete(document);

        Ok(())
    }
}

pub trait JSONMailValue {
    fn parse_header(
        self,
        builder: &mut MessageBuilder,
        header: HeaderName,
        form: MailHeaderForm,
    ) -> jmap::error::set::Result<()>;
    fn parse_body_structure<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
    ) -> jmap::error::set::Result<MimePart>
    where
        T: for<'x> Store<'x> + 'static;
    fn parse_body_part<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
        implicit_type: Option<&'static str>,
        strict_implicit_type: bool,
    ) -> jmap::error::set::Result<(MimePart, Option<Vec<JSONValue>>)>
    where
        T: for<'x> Store<'x> + 'static;
    fn parse_body_parts<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
        implicit_type: Option<&'static str>,
        strict_implicit_type: bool,
    ) -> jmap::error::set::Result<Vec<MimePart>>
    where
        T: for<'x> Store<'x> + 'static;
    fn parse_json_string(self) -> jmap::error::set::Result<String>;
    fn parse_json_date(self) -> jmap::error::set::Result<u64>;
    fn parse_json_string_list(self) -> jmap::error::set::Result<Vec<String>>;
    fn parse_json_addresses(self) -> jmap::error::set::Result<Vec<Address>>;
    fn parse_json_grouped_addresses(self) -> jmap::error::set::Result<Vec<Address>>;
}

impl JSONMailValue for JSONValue {
    fn parse_header(
        self,
        builder: &mut MessageBuilder,
        header: HeaderName,
        form: MailHeaderForm,
    ) -> jmap::error::set::Result<()> {
        match form {
            MailHeaderForm::Raw => {
                builder.header(header.unwrap(), Raw::new(self.parse_json_string()?))
            }
            MailHeaderForm::Text => {
                builder.header(header.unwrap(), Text::new(self.parse_json_string()?))
            }
            MailHeaderForm::Addresses => {
                builder.header(header.unwrap(), Address::List(self.parse_json_addresses()?))
            }
            MailHeaderForm::GroupedAddresses => builder.header(
                header.unwrap(),
                Address::List(self.parse_json_grouped_addresses()?),
            ),
            MailHeaderForm::MessageIds => builder.header(
                header.unwrap(),
                MessageId::from(self.parse_json_string_list()?),
            ),
            MailHeaderForm::Date => {
                builder.header(header.unwrap(), Date::new(self.parse_json_date()? as i64))
            }
            MailHeaderForm::URLs => {
                builder.header(header.unwrap(), URL::from(self.parse_json_string_list()?))
            }
        }
        Ok(())
    }

    fn parse_body_structure<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
    ) -> jmap::error::set::Result<MimePart>
    where
        T: for<'x> Store<'x> + 'static,
    {
        let (mut mime_part, sub_parts) = self.parse_body_part(helper, body_values, None, false)?;

        if let Some(sub_parts) = sub_parts {
            let mut stack = Vec::new();
            let mut it = sub_parts.into_iter();

            loop {
                while let Some(part) = it.next() {
                    let (sub_mime_part, sub_parts) =
                        part.parse_body_part(helper, body_values, None, false)?;
                    if let Some(sub_parts) = sub_parts {
                        stack.push((mime_part, it));
                        mime_part = sub_mime_part;
                        it = sub_parts.into_iter();
                    } else {
                        mime_part.add_part(sub_mime_part);
                    }
                }
                if let Some((mut prev_mime_part, prev_it)) = stack.pop() {
                    prev_mime_part.add_part(mime_part);
                    mime_part = prev_mime_part;
                    it = prev_it;
                } else {
                    break;
                }
            }
        }

        Ok(mime_part)
    }

    fn parse_body_part<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
        implicit_type: Option<&'static str>,
        strict_implicit_type: bool,
    ) -> jmap::error::set::Result<(MimePart, Option<Vec<JSONValue>>)>
    where
        T: for<'x> Store<'x> + 'static,
    {
        let mut part = self.unwrap_object().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected an object in body part list.".to_string(),
            )
        })?;

        let content_type = part
            .remove("type")
            .and_then(|v| v.unwrap_string())
            .unwrap_or_else(|| implicit_type.unwrap_or("text/plain").to_string());

        if strict_implicit_type && implicit_type.unwrap() != content_type {
            return Err(SetError::new(
                SetErrorType::InvalidProperties,
                format!(
                    "Expected exactly body part of type \"{}\"",
                    implicit_type.unwrap()
                ),
            ));
        }

        let is_multipart = content_type.starts_with("multipart/");
        let mut mime_part = MimePart {
            headers: BTreeMap::new(),
            contents: if is_multipart {
                BodyPart::Multipart(vec![])
            } else if let Some(part_id) = part.remove("partId").and_then(|v| v.unwrap_string()) {
                BodyPart::Text( body_values.as_mut()
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            "Missing \"bodyValues\" object containing partId.".to_string(),
                        )
                    })?
                    .remove(&part_id)
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            format!("Missing body value for partId \"{}\"", part_id),
                        )
                    })?
                    .unwrap_object()
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            format!("Expected a bodyValues object defining partId \"{}\"", part_id),
                        )
                    })?
                    .remove("value")
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            format!("Missing \"value\" field in bodyValues object defining partId \"{}\"", part_id),
                        )
                    })?
                    .unwrap_string()
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            format!("Expected a string \"value\" field in bodyValues object defining partId \"{}\"", part_id),
                        )
                    })?)
            } else if let Some(blob_id) = part.remove("blobId").and_then(|v| v.unwrap_string()) {
                BodyPart::Binary(
                    helper
                        .store
                        .blob_jmap_get(
                            helper.account_id,
                            &JMAPBlob::from_jmap_string(&blob_id).ok_or_else(|| {
                                SetError::new(SetErrorType::BlobNotFound, "Failed to parse blobId")
                            })?,
                            get_message_part,
                        )
                        .map_err(|_| {
                            SetError::new(SetErrorType::BlobNotFound, "Failed to fetch blob.")
                        })?
                        .ok_or_else(|| {
                            SetError::new(
                                SetErrorType::BlobNotFound,
                                "blobId does not exist on this server.",
                            )
                        })?,
                )
            } else {
                return Err(SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected a \"partId\" or \"blobId\" field in body part.".to_string(),
                ));
            },
        };

        let mut content_type = ContentType::new(content_type);
        if !is_multipart {
            if content_type.c_type.starts_with("text/") {
                if matches!(mime_part.contents, BodyPart::Text(_)) {
                    content_type
                        .attributes
                        .insert("charset".into(), "utf-8".into());
                } else if let Some(charset) = part.remove("charset") {
                    content_type.attributes.insert(
                        "charset".into(),
                        charset
                            .to_string()
                            .ok_or_else(|| {
                                SetError::new(
                                    SetErrorType::InvalidProperties,
                                    "Expected a string value for \"charset\" field.".to_string(),
                                )
                            })?
                            .into(),
                    );
                };
            }

            match (
                part.remove("disposition").and_then(|v| v.unwrap_string()),
                part.remove("name").and_then(|v| v.unwrap_string()),
            ) {
                (Some(disposition), Some(filename)) => {
                    mime_part.headers.insert(
                        "Content-Disposition".into(),
                        ContentType::new(disposition)
                            .attribute("filename", filename)
                            .into(),
                    );
                }
                (Some(disposition), None) => {
                    mime_part.headers.insert(
                        "Content-Disposition".into(),
                        ContentType::new(disposition).into(),
                    );
                }
                (None, Some(filename)) => {
                    content_type.attributes.insert("name".into(), filename);
                }
                (None, None) => (),
            };

            if let Some(languages) = part.remove("language").and_then(|v| v.unwrap_array()) {
                mime_part.headers.insert(
                    "Content-Language".into(),
                    Text::new(
                        languages
                            .iter()
                            .filter_map(|v| v.to_string())
                            .collect::<Vec<&str>>()
                            .join(","),
                    )
                    .into(),
                );
            }

            if let Some(cid) = part.remove("cid").and_then(|v| v.unwrap_string()) {
                mime_part
                    .headers
                    .insert("Content-ID".into(), MessageId::new(cid).into());
            }

            if let Some(location) = part.remove("location").and_then(|v| v.unwrap_string()) {
                mime_part
                    .headers
                    .insert("Content-Location".into(), Text::new(location).into());
            }
        }

        mime_part
            .headers
            .insert("Content-Type".into(), content_type.into());
        let mut sub_parts = None;

        for (property, value) in part {
            if property.starts_with("header:") {
                match property.split(':').nth(1) {
                    Some(header_name) if !header_name.is_empty() => {
                        mime_part.headers.insert(
                            header_name.into(),
                            Raw::new(value.unwrap_string().ok_or_else(|| {
                                SetError::new(
                                    SetErrorType::InvalidProperties,
                                    format!("Expected a string value for \"{}\" field.", property),
                                )
                            })?)
                            .into(),
                        );
                    }
                    _ => (),
                }
            } else if property == "headers" {
                if let Some(headers) = value.unwrap_array() {
                    for header in headers {
                        let mut header = header.unwrap_object().ok_or_else(|| {
                            SetError::new(
                                SetErrorType::InvalidProperties,
                                "Expected an object for \"headers\" field.".to_string(),
                            )
                        })?;
                        mime_part.headers.insert(
                            header
                                .remove("name")
                                .and_then(|v| v.unwrap_string())
                                .ok_or_else(|| {
                                    SetError::new(
                                        SetErrorType::InvalidProperties,
                                        "Expected a string value for \"name\" field in \"headers\" field."
                                            .to_string(),
                                    )
                                })?
                                ,
                            Raw::new(
                                header
                                    .remove("value")
                                    .and_then(|v| v.unwrap_string())
                                    .ok_or_else(|| {
                                        SetError::new(
                                        SetErrorType::InvalidProperties,
                                        "Expected a string value for \"value\" field in \"headers\" field."
                                            .to_string(),
                                    )
                                    })?,
                            )
                            .into(),
                        );
                    }
                }
            } else if property == "subParts" {
                sub_parts = value.unwrap_array();
            }
        }

        Ok((mime_part, if is_multipart { sub_parts } else { None }))
    }

    fn parse_body_parts<T>(
        self,
        helper: &SetObjectHelper<T, SetMailHelper>,
        body_values: &mut Option<HashMap<String, JSONValue>>,
        implicit_type: Option<&'static str>,
        strict_implicit_type: bool,
    ) -> jmap::error::set::Result<Vec<MimePart>>
    where
        T: for<'x> Store<'x> + 'static,
    {
        let parts = self.unwrap_array().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected an array containing body part.".to_string(),
            )
        })?;

        let mut result = Vec::with_capacity(parts.len());
        for part in parts {
            result.push(
                part.parse_body_part(helper, body_values, implicit_type, strict_implicit_type)?
                    .0,
            );
        }

        Ok(result)
    }

    fn parse_json_string(self) -> jmap::error::set::Result<String> {
        self.unwrap_string().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected a String property.".to_string(),
            )
        })
    }

    fn parse_json_date(self) -> jmap::error::set::Result<u64> {
        Ok(
            DateTime::parse_from_rfc3339(self.to_string().ok_or_else(|| {
                SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected a UTCDate property.".to_string(),
                )
            })?)
            .map_err(|_| {
                SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected a valid UTCDate property.".to_string(),
                )
            })?
            .timestamp() as u64,
        )
    }

    fn parse_json_string_list(self) -> jmap::error::set::Result<Vec<String>> {
        let value = self.unwrap_array().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected a String array.".to_string(),
            )
        })?;

        let mut list = Vec::with_capacity(value.len());
        for v in value {
            list.push(v.unwrap_string().ok_or_else(|| {
                SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected a String array.".to_string(),
                )
            })?);
        }

        Ok(list)
    }

    fn parse_json_addresses(self) -> jmap::error::set::Result<Vec<Address>> {
        let value = self.unwrap_array().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected an array with EmailAddress objects.".to_string(),
            )
        })?;

        let mut result = Vec::with_capacity(value.len());
        for addr in value {
            let mut addr = addr.unwrap_object().ok_or_else(|| {
                SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected an array containing EmailAddress objects.".to_string(),
                )
            })?;
            result.push(Address::new_address(
                addr.remove("name").and_then(|n| n.unwrap_string()),
                addr.remove("email")
                    .and_then(|n| n.unwrap_string())
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            "Missing 'email' field in EmailAddress object.".to_string(),
                        )
                    })?,
            ));
        }

        Ok(result)
    }

    fn parse_json_grouped_addresses<'x>(self) -> jmap::error::set::Result<Vec<Address>> {
        let value = self.unwrap_array().ok_or_else(|| {
            SetError::new(
                SetErrorType::InvalidProperties,
                "Expected an array with EmailAddressGroup objects.".to_string(),
            )
        })?;

        let mut result = Vec::with_capacity(value.len());
        for addr in value {
            let mut addr = addr.unwrap_object().ok_or_else(|| {
                SetError::new(
                    SetErrorType::InvalidProperties,
                    "Expected an array containing EmailAddressGroup objects.".to_string(),
                )
            })?;
            result.push(Address::new_group(
                addr.remove("name").and_then(|n| n.unwrap_string()),
                addr.remove("addresses")
                    .ok_or_else(|| {
                        SetError::new(
                            SetErrorType::InvalidProperties,
                            "Missing 'addresses' field in EmailAddressGroup object.".to_string(),
                        )
                    })?
                    .parse_json_addresses()?,
            ));
        }

        Ok(result)
    }
}