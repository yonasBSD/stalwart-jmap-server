use std::{collections::HashMap, fmt::Display};

use jmap::{
    id::{blob::JMAPBlob, jmap::JMAPId},
    jmap_store::orm::{self, Indexable},
    request::ResultReference,
};
use serde::{Deserialize, Serialize};
use store::{
    chrono::{DateTime, Utc},
    FieldId,
};

#[derive(Debug, Clone, Default)]
pub struct EmailSubmission {
    pub properties: HashMap<Property, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Id {
        value: JMAPId,
    },
    Text {
        value: String,
    },
    DateTime {
        value: DateTime<Utc>,
    },
    UndoStatus {
        value: UndoStatus,
    },
    DeliveryStatus {
        value: HashMap<String, DeliveryStatus>,
    },
    Envelope {
        value: Envelope,
    },
    BlobIds {
        value: Vec<JMAPBlob>,
    },
    IdReference {
        value: String,
    },
    ResultReference {
        value: ResultReference,
    },
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(rename = "mailFrom")]
    pub mail_from: Address,

    #[serde(rename = "rcptTo")]
    pub rcpt_to: Vec<Address>,
}

impl Envelope {
    pub fn new(email: String) -> Self {
        Envelope {
            mail_from: Address {
                email,
                parameters: None,
            },
            rcpt_to: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub email: String,
    pub parameters: Option<HashMap<String, Option<String>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndoStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "final")]
    Final,
    #[serde(rename = "canceled")]
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryStatus {
    #[serde(rename = "smtpReply")]
    pub smtp_reply: String,

    #[serde(rename = "delivered")]
    pub delivered: Delivered,

    #[serde(rename = "displayed")]
    pub displayed: Displayed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delivered {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Displayed {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "yes")]
    Yes,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[repr(u8)]
pub enum Property {
    Id = 0,
    IdentityId = 1,
    EmailId = 2,
    ThreadId = 3,
    Envelope = 4,
    SendAt = 5,
    UndoStatus = 6,
    DeliveryStatus = 7,
    DsnBlobIds = 8,
    MdnBlobIds = 9,
    Invalid = 10,
}

impl Property {
    pub fn parse(value: &str) -> Property {
        match value {
            "id" => Property::Id,
            "identityId" => Property::IdentityId,
            "emailId" => Property::EmailId,
            "threadId" => Property::ThreadId,
            "envelope" => Property::Envelope,
            "sendAt" => Property::SendAt,
            "undoStatus" => Property::UndoStatus,
            "deliveryStatus" => Property::DeliveryStatus,
            "dsnBlobIds" => Property::DsnBlobIds,
            "mdnBlobIds" => Property::MdnBlobIds,
            _ => Property::Invalid,
        }
    }
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::IdentityId => write!(f, "identityId"),
            Property::EmailId => write!(f, "emailId"),
            Property::ThreadId => write!(f, "threadId"),
            Property::Envelope => write!(f, "envelope"),
            Property::SendAt => write!(f, "sendAt"),
            Property::UndoStatus => write!(f, "undoStatus"),
            Property::DeliveryStatus => write!(f, "deliveryStatus"),
            Property::DsnBlobIds => write!(f, "dsnBlobIds"),
            Property::MdnBlobIds => write!(f, "mdnBlobIds"),
            Property::Invalid => Ok(()),
        }
    }
}

impl From<Property> for FieldId {
    fn from(property: Property) -> Self {
        property as FieldId
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Filter {
    IdentityIds {
        #[serde(rename = "identityIds")]
        value: Vec<JMAPId>,
    },
    EmailIds {
        #[serde(rename = "emailIds")]
        value: Vec<JMAPId>,
    },
    ThreadIds {
        #[serde(rename = "threadIds")]
        value: Vec<JMAPId>,
    },
    UndoStatus {
        #[serde(rename = "undoStatus")]
        value: UndoStatus,
    },
    Before {
        #[serde(rename = "before")]
        value: DateTime<Utc>,
    },
    After {
        #[serde(rename = "after")]
        value: DateTime<Utc>,
    },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "property")]
pub enum Comparator {
    #[serde(rename = "emailId")]
    EmailId,
    #[serde(rename = "threadId")]
    ThreadId,
    #[serde(rename = "sentAt")]
    SentAt,
}

impl Indexable for Value {
    fn index_as(&self) -> orm::Value<Self> {
        match self {
            Value::Id { value } => u64::from(value).into(),
            Value::DateTime { value } => (value.timestamp() as u64).into(),
            Value::UndoStatus { value } => match value {
                UndoStatus::Pending => "p".to_string().into(),
                UndoStatus::Final => "f".to_string().into(),
                UndoStatus::Canceled => "c".to_string().into(),
            },
            _ => orm::Value::Null,
        }
    }
}