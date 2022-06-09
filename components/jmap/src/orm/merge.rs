use store::core::document::Document;
use store::nlp::Language;
use store::write::options::{IndexOptions, Options};

use crate::error::set::SetError;

use super::{Index, Object, TinyORM, Value};

impl<T> TinyORM<T>
where
    T: Object + 'static,
{
    pub fn merge_validate(
        self,
        document: &mut Document,
        changes: TinyORM<T>,
    ) -> crate::error::set::Result<bool, T::Property> {
        for property in T::required() {
            if changes
                .properties
                .get(property)
                .map(|v| v.is_empty())
                .unwrap_or_else(|| self.properties.get(property).is_none())
            {
                return Err(SetError::invalid_property(
                    property.clone(),
                    "Property cannot be empty.".to_string(),
                ));
            }
        }
        self.merge(document, changes).map_err(|err| err.into())
    }

    pub fn merge(mut self, document: &mut Document, changes: Self) -> store::Result<bool> {
        let indexed = T::indexed();
        let mut has_changes = false;

        for (property, new_value) in changes.properties {
            let (is_indexed, index_options) = indexed
                .iter()
                .filter_map(|(p, index_type)| {
                    if p == &property {
                        Some((true, index_type))
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or((false, &0));

            if let Some(current_value) = self.properties.get(&property) {
                if current_value == &new_value {
                    continue;
                } else if is_indexed {
                    match current_value.index_as() {
                        Index::Text(current_value) => {
                            document.text(
                                property.clone(),
                                current_value,
                                Language::Unknown,
                                (*index_options).clear(),
                            );
                        }
                        Index::Integer(current_value) => {
                            document.number(
                                property.clone(),
                                current_value,
                                (*index_options).clear(),
                            );
                        }
                        Index::LongInteger(current_value) => {
                            document.number(
                                property.clone(),
                                current_value,
                                (*index_options).clear(),
                            );
                        }
                        Index::TextList(current_value) => {
                            // Add to the index the new strings and delete the ones that
                            // were removed from the list.
                            if let Index::TextList(new_value_) = new_value.index_as() {
                                for item in &current_value {
                                    if !new_value_.contains(item) {
                                        document.text(
                                            property.clone(),
                                            item.clone(),
                                            Language::Unknown,
                                            (*index_options).clear(),
                                        );
                                    }
                                }
                                for item in new_value_ {
                                    if !current_value.contains(&item) {
                                        document.text(
                                            property.clone(),
                                            item,
                                            Language::Unknown,
                                            *index_options,
                                        );
                                    }
                                }
                                self.properties.insert(property, new_value);
                            } else {
                                for item in current_value {
                                    document.text(
                                        property.clone(),
                                        item,
                                        Language::Unknown,
                                        (*index_options).clear(),
                                    );
                                }
                                self.properties.remove(&property);
                            }
                            has_changes = true;
                            continue;
                        }
                        Index::IntegerList(current_value) => {
                            // Add to the index the new integers and delete the ones that
                            // were removed from the list.

                            if let Index::IntegerList(new_value_) = new_value.index_as() {
                                for item in &current_value {
                                    if !new_value_.contains(item) {
                                        document.number(
                                            property.clone(),
                                            *item,
                                            (*index_options).clear(),
                                        );
                                    }
                                }
                                for item in new_value_ {
                                    if !current_value.contains(&item) {
                                        document.number(property.clone(), item, *index_options);
                                    }
                                }
                                self.properties.insert(property, new_value);
                            } else {
                                for item in current_value {
                                    document.number(
                                        property.clone(),
                                        item,
                                        (*index_options).clear(),
                                    );
                                }
                                self.properties.remove(&property);
                            }
                            has_changes = true;
                            continue;
                        }
                        Index::Null => (),
                    }
                }
            }

            let do_insert = if is_indexed {
                match new_value.index_as() {
                    Index::Text(value) => {
                        document.text(property.clone(), value, Language::Unknown, *index_options);
                        true
                    }
                    Index::TextList(value) => {
                        for item in value {
                            document.text(
                                property.clone(),
                                item,
                                Language::Unknown,
                                *index_options,
                            );
                        }
                        true
                    }
                    Index::Integer(value) => {
                        document.number(property.clone(), value, *index_options);
                        true
                    }
                    Index::IntegerList(value) => {
                        for item in value {
                            document.number(property.clone(), item, *index_options);
                        }
                        true
                    }
                    Index::LongInteger(value) => {
                        document.number(property.clone(), value, *index_options);
                        true
                    }
                    Index::Null => false,
                }
            } else {
                !new_value.is_empty()
            };

            if do_insert {
                self.properties.insert(property, new_value);
            } else {
                self.properties.remove(&property);
            }

            if !has_changes {
                has_changes = true;
            }
        }

        if self.tags != changes.tags {
            for (property, tags) in &self.tags {
                if let Some(changed_tags) = changes.tags.get(property) {
                    if tags != changed_tags {
                        for tag in tags {
                            if !changed_tags.contains(tag) {
                                document.tag(
                                    property.clone(),
                                    tag.clone(),
                                    IndexOptions::new().clear(),
                                );
                            }
                        }
                    }
                }
            }

            for (property, changed_tags) in &changes.tags {
                if let Some(tags) = self.tags.get(property) {
                    if changed_tags != tags {
                        for changed_tag in changed_tags {
                            if !tags.contains(changed_tag) {
                                document.tag(
                                    property.clone(),
                                    changed_tag.clone(),
                                    IndexOptions::new(),
                                );
                            }
                        }
                    }
                } else {
                    for changed_tag in changed_tags {
                        document.tag(property.clone(), changed_tag.clone(), IndexOptions::new());
                    }
                }
            }

            self.tags = changes.tags;

            if !has_changes {
                has_changes = true;
            }
        }

        if self.acls != changes.acls {
            for acl in &self.acls {
                if !changes.acls.iter().any(|ca| ca.id == acl.id) {
                    document.acl(acl.clone(), IndexOptions::new().clear());
                }
            }

            for acl in &changes.acls {
                if !self.acls.contains(acl) {
                    document.acl(acl.clone(), IndexOptions::new());
                }
            }

            if !has_changes {
                has_changes = true;
            }
        }

        if has_changes {
            self.insert_orm(document)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}