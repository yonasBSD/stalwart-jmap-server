use std::{collections::HashMap, fmt};

use jmap::{
    id::{blob::JMAPBlob, jmap::JMAPId},
    protocol::json_pointer::JSONPointer,
    request::MaybeIdReference,
};
use serde::{ser::SerializeMap, Deserialize, Serialize};
use store::chrono::{DateTime, Utc};

use super::schema::{
    BodyProperty, Email, EmailAddress, EmailBodyPart, EmailHeader, EmailValue, HeaderForm,
    HeaderProperty, Keyword, Property,
};

// Email de/serialization
impl Serialize for Email {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(self.properties.len().into())?;

        for (name, value) in &self.properties {
            match value {
                EmailValue::Id { value } => map.serialize_entry(name, value)?,
                EmailValue::Blob { value } => map.serialize_entry(name, value)?,
                EmailValue::Size { value } => map.serialize_entry(name, value)?,
                EmailValue::Bool { value } => map.serialize_entry(name, value)?,
                EmailValue::Keywords { value, .. } => map.serialize_entry(name, value)?,
                EmailValue::MailboxIds { value, .. } => map.serialize_entry(name, value)?,
                EmailValue::ResultReference { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyPart { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyPartList { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyValues { value } => map.serialize_entry(name, value)?,
                EmailValue::Text { value } => map.serialize_entry(name, value)?,
                EmailValue::TextList { value } => map.serialize_entry(name, value)?,
                EmailValue::TextListMany { value } => map.serialize_entry(name, value)?,
                EmailValue::Date { value } => map.serialize_entry(name, value)?,
                EmailValue::DateList { value } => map.serialize_entry(name, value)?,
                EmailValue::Addresses { value } => map.serialize_entry(name, value)?,
                EmailValue::AddressesList { value } => map.serialize_entry(name, value)?,
                EmailValue::GroupedAddresses { value } => map.serialize_entry(name, value)?,
                EmailValue::GroupedAddressesList { value } => map.serialize_entry(name, value)?,
                EmailValue::Headers { value } => map.serialize_entry(name, value)?,
            }
        }

        map.end()
    }
}
struct EmailVisitor;

impl<'de> serde::de::Visitor<'de> for EmailVisitor {
    type Value = Email;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP e-mail object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut properties: HashMap<Property, EmailValue> = HashMap::new();

        while let Some(key) = map.next_key::<&str>()? {
            match key {
                "keywords" => {
                    if let Some(value) = map.next_value::<Option<HashMap<Keyword, bool>>>()? {
                        properties.insert(
                            Property::Keywords,
                            EmailValue::Keywords { value, set: true },
                        );
                    }
                }
                "mailboxIds" => {
                    if let Some(value) =
                        map.next_value::<Option<HashMap<MaybeIdReference, bool>>>()?
                    {
                        properties.insert(
                            Property::MailboxIds,
                            EmailValue::MailboxIds { value, set: true },
                        );
                    }
                }
                "#mailboxIds" => {
                    properties.insert(
                        Property::MailboxIds,
                        EmailValue::ResultReference {
                            value: map.next_value()?,
                        },
                    );
                }
                "messageId" => {
                    if let Some(value) = map.next_value::<Option<Vec<String>>>()? {
                        properties.insert(Property::MessageId, EmailValue::TextList { value });
                    }
                }
                "inReplyTo" => {
                    if let Some(value) = map.next_value::<Option<Vec<String>>>()? {
                        properties.insert(Property::InReplyTo, EmailValue::TextList { value });
                    }
                }
                "references" => {
                    if let Some(value) = map.next_value::<Option<Vec<String>>>()? {
                        properties.insert(Property::References, EmailValue::TextList { value });
                    }
                }
                "sender" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::Sender, EmailValue::Addresses { value });
                    }
                }
                "from" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::From, EmailValue::Addresses { value });
                    }
                }
                "to" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::To, EmailValue::Addresses { value });
                    }
                }
                "cc" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::Cc, EmailValue::Addresses { value });
                    }
                }
                "bcc" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::Bcc, EmailValue::Addresses { value });
                    }
                }
                "replyTo" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailAddress>>>()? {
                        properties.insert(Property::ReplyTo, EmailValue::Addresses { value });
                    }
                }
                "subject" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(Property::Subject, EmailValue::Text { value });
                    }
                }
                "sentAt" => {
                    if let Some(value) = map.next_value::<Option<DateTime<Utc>>>()? {
                        properties.insert(Property::SentAt, EmailValue::Date { value });
                    }
                }
                "receivedAt" => {
                    if let Some(value) = map.next_value::<Option<DateTime<Utc>>>()? {
                        properties.insert(Property::ReceivedAt, EmailValue::Date { value });
                    }
                }
                "preview" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(Property::Preview, EmailValue::Text { value });
                    }
                }
                "textBody" => {
                    properties.insert(
                        Property::TextBody,
                        EmailValue::BodyPartList {
                            value: map.next_value()?,
                        },
                    );
                }
                "htmlBody" => {
                    properties.insert(
                        Property::HtmlBody,
                        EmailValue::BodyPartList {
                            value: map.next_value()?,
                        },
                    );
                }
                "attachments" => {
                    properties.insert(
                        Property::Attachments,
                        EmailValue::BodyPartList {
                            value: map.next_value()?,
                        },
                    );
                }
                "hasAttachment" => {
                    if let Some(value) = map.next_value::<Option<bool>>()? {
                        properties.insert(Property::HasAttachment, EmailValue::Bool { value });
                    }
                }
                "id" => {
                    if let Some(value) = map.next_value::<Option<JMAPId>>()? {
                        properties.insert(Property::Id, EmailValue::Id { value });
                    }
                }
                "blobId" => {
                    if let Some(value) = map.next_value::<Option<JMAPBlob>>()? {
                        properties.insert(Property::BlobId, EmailValue::Blob { value });
                    }
                }
                "threadId" => {
                    if let Some(value) = map.next_value::<Option<JMAPId>>()? {
                        properties.insert(Property::ThreadId, EmailValue::Id { value });
                    }
                }
                "size" => {
                    if let Some(value) = map.next_value::<Option<usize>>()? {
                        properties.insert(Property::Size, EmailValue::Size { value });
                    }
                }
                "bodyValues" => {
                    properties.insert(
                        Property::BodyValues,
                        EmailValue::BodyValues {
                            value: map.next_value()?,
                        },
                    );
                }
                "bodyStructure" => {
                    properties.insert(
                        Property::BodyStructure,
                        EmailValue::BodyPart {
                            value: map.next_value()?,
                        },
                    );
                }
                _ if key.starts_with("header:") => {
                    if let Some(header) = HeaderProperty::parse(key) {
                        let header_value = match header.form {
                            HeaderForm::Raw | HeaderForm::Text => {
                                if header.all {
                                    EmailValue::TextList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Text {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::Addresses => {
                                if header.all {
                                    EmailValue::AddressesList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Addresses {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::GroupedAddresses => {
                                if header.all {
                                    EmailValue::GroupedAddressesList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::GroupedAddresses {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::MessageIds | HeaderForm::URLs => {
                                if header.all {
                                    EmailValue::TextListMany {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::TextList {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::Date => {
                                if header.all {
                                    EmailValue::DateList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Date {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                        };
                        properties.insert(Property::Header(header), header_value);
                    }
                }
                _ => {
                    if let Some(pointer) = JSONPointer::parse(key) {
                        match pointer {
                            JSONPointer::Path(path) if path.len() == 2 => {
                                if let (
                                    Some(JSONPointer::String(property)),
                                    Some(JSONPointer::String(id)),
                                ) = (path.get(0), path.get(1))
                                {
                                    let value = map.next_value::<Option<bool>>()?.unwrap_or(false);
                                    match Property::parse(property) {
                                        Property::MailboxIds => {
                                            if let Some(id) = JMAPId::parse(id) {
                                                properties
                                                    .entry(Property::MailboxIds)
                                                    .or_insert_with(|| EmailValue::MailboxIds {
                                                        value: HashMap::new(),
                                                        set: false,
                                                    })
                                                    .get_mailbox_ids()
                                                    .unwrap()
                                                    .insert(MaybeIdReference::Value(id), value);
                                            }
                                        }
                                        Property::Keywords => {
                                            properties
                                                .entry(Property::MailboxIds)
                                                .or_insert_with(|| EmailValue::Keywords {
                                                    value: HashMap::new(),
                                                    set: false,
                                                })
                                                .get_keywords()
                                                .unwrap()
                                                .insert(Keyword::parse(id), value);
                                        }
                                        _ => (),
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
        }

        Ok(Email { properties })
    }
}

impl<'de> Deserialize<'de> for Email {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(EmailVisitor)
    }
}

// EmailBodyPart de/serialization
impl Serialize for EmailBodyPart {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(self.properties.len().into())?;

        for (name, value) in &self.properties {
            match value {
                EmailValue::Id { value } => map.serialize_entry(name, value)?,
                EmailValue::Blob { value } => map.serialize_entry(name, value)?,
                EmailValue::Size { value } => map.serialize_entry(name, value)?,
                EmailValue::Bool { value } => map.serialize_entry(name, value)?,
                EmailValue::Keywords { value, .. } => map.serialize_entry(name, value)?,
                EmailValue::MailboxIds { value, .. } => map.serialize_entry(name, value)?,
                EmailValue::ResultReference { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyPart { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyPartList { value } => map.serialize_entry(name, value)?,
                EmailValue::BodyValues { value } => map.serialize_entry(name, value)?,
                EmailValue::Text { value } => map.serialize_entry(name, value)?,
                EmailValue::TextList { value } => map.serialize_entry(name, value)?,
                EmailValue::TextListMany { value } => map.serialize_entry(name, value)?,
                EmailValue::Date { value } => map.serialize_entry(name, value)?,
                EmailValue::DateList { value } => map.serialize_entry(name, value)?,
                EmailValue::Addresses { value } => map.serialize_entry(name, value)?,
                EmailValue::AddressesList { value } => map.serialize_entry(name, value)?,
                EmailValue::GroupedAddresses { value } => map.serialize_entry(name, value)?,
                EmailValue::GroupedAddressesList { value } => map.serialize_entry(name, value)?,
                EmailValue::Headers { value } => map.serialize_entry(name, value)?,
            }
        }

        map.end()
    }
}
struct EmailBodyPartVisitor;

impl<'de> serde::de::Visitor<'de> for EmailBodyPartVisitor {
    type Value = EmailBodyPart;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP EmailBodyPart object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut properties: HashMap<BodyProperty, EmailValue> = HashMap::new();

        while let Some(key) = map.next_key::<&str>()? {
            match key {
                "partId" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::PartId, EmailValue::Text { value });
                    }
                }
                "blobId" => {
                    if let Some(value) = map.next_value::<Option<JMAPBlob>>()? {
                        properties.insert(BodyProperty::BlobId, EmailValue::Blob { value });
                    }
                }
                "size" => {
                    if let Some(value) = map.next_value::<Option<usize>>()? {
                        properties.insert(BodyProperty::Size, EmailValue::Size { value });
                    }
                }
                "name" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailHeader>>>()? {
                        properties.insert(BodyProperty::Headers, EmailValue::Headers { value });
                    }
                }
                "type" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::Type, EmailValue::Text { value });
                    }
                }
                "charset" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::Charset, EmailValue::Text { value });
                    }
                }
                "disposition" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::Disposition, EmailValue::Text { value });
                    }
                }
                "cid" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::Cid, EmailValue::Text { value });
                    }
                }
                "language" => {
                    if let Some(value) = map.next_value::<Option<Vec<String>>>()? {
                        properties.insert(BodyProperty::Language, EmailValue::TextList { value });
                    }
                }
                "location" => {
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        properties.insert(BodyProperty::Location, EmailValue::Text { value });
                    }
                }
                "subParts" => {
                    if let Some(value) = map.next_value::<Option<Vec<EmailBodyPart>>>()? {
                        properties
                            .insert(BodyProperty::Subparts, EmailValue::BodyPartList { value });
                    }
                }
                _ if key.starts_with("header:") => {
                    if let Some(header) = HeaderProperty::parse(key) {
                        let header_value = match header.form {
                            HeaderForm::Raw | HeaderForm::Text => {
                                if header.all {
                                    EmailValue::TextList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Text {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::Addresses => {
                                if header.all {
                                    EmailValue::AddressesList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Addresses {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::GroupedAddresses => {
                                if header.all {
                                    EmailValue::GroupedAddressesList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::GroupedAddresses {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::MessageIds | HeaderForm::URLs => {
                                if header.all {
                                    EmailValue::TextListMany {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::TextList {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                            HeaderForm::Date => {
                                if header.all {
                                    EmailValue::DateList {
                                        value: map.next_value()?,
                                    }
                                } else {
                                    EmailValue::Date {
                                        value: map.next_value()?,
                                    }
                                }
                            }
                        };
                        properties.insert(BodyProperty::Header(header), header_value);
                    }
                }
                _ => (),
            }
        }

        Ok(EmailBodyPart { properties })
    }
}

impl<'de> Deserialize<'de> for EmailBodyPart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(EmailBodyPartVisitor)
    }
}

// Property de/serialization
impl Serialize for Property {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
struct PropertyVisitor;

impl<'de> serde::de::Visitor<'de> for PropertyVisitor {
    type Value = Property;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP e-mail property")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Property::parse(v))
    }
}

impl<'de> Deserialize<'de> for Property {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(PropertyVisitor)
    }
}

// BodyProperty de/serialization
impl Serialize for BodyProperty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
struct BodyPropertyVisitor;

impl<'de> serde::de::Visitor<'de> for BodyPropertyVisitor {
    type Value = BodyProperty;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP body property")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        BodyProperty::parse(v).ok_or_else(|| E::custom(format!("Invalid body property: {}", v)))
    }
}

impl<'de> Deserialize<'de> for BodyProperty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(BodyPropertyVisitor)
    }
}

// HeaderProperty de/serialization
impl Serialize for HeaderProperty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
struct HeaderPropertyVisitor;

impl<'de> serde::de::Visitor<'de> for HeaderPropertyVisitor {
    type Value = HeaderProperty;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP header property")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        HeaderProperty::parse(v).ok_or_else(|| E::custom(format!("Invalid property: {}", v)))
    }
}

impl<'de> Deserialize<'de> for HeaderProperty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(HeaderPropertyVisitor)
    }
}

// Keyword de/serialization
impl Serialize for Keyword {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
struct KeywordVisitor;

impl<'de> serde::de::Visitor<'de> for KeywordVisitor {
    type Value = Keyword;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP keyword")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Keyword::parse(v))
    }
}

impl<'de> Deserialize<'de> for Keyword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(KeywordVisitor)
    }
}