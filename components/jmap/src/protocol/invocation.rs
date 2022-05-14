use std::fmt::Display;

use store::{config::jmap::JMAPConfig, core::collection::Collection, AccountId};

use crate::{
    error::method::MethodError,
    request::{
        changes::ChangesRequest, get::GetRequest, import::ImportRequest, parse::ParseRequest,
        query::QueryRequest, query_changes::QueryChangesRequest, set::SetRequest,
    },
};

use super::{json::JSONValue, response::Response};

#[derive(Debug)]
pub struct Invocation {
    pub obj: Object,
    pub call: Method,
    pub account_id: AccountId,
}

#[derive(Debug, serde::Serialize, Eq, PartialEq, Hash, Clone)]
pub enum Object {
    Core,
    Mailbox,
    Thread,
    Email,
    SearchSnippet,
    Identity,
    EmailSubmission,
    VacationResponse,
    PushSubscription,
}

#[derive(Debug)]
pub enum Method {
    Echo(JSONValue),
    Get(GetRequest),
    Set(SetRequest),
    Query(QueryRequest),
    QueryChanges(QueryChangesRequest),
    Changes(ChangesRequest),
    Import(ImportRequest),
    Parse(ParseRequest),
}

impl Object {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "Core" => Some(Object::Core),
            "Mailbox" => Some(Object::Mailbox),
            "Thread" => Some(Object::Thread),
            "Email" => Some(Object::Email),
            "SearchSnippet" => Some(Object::SearchSnippet),
            "Identity" => Some(Object::Identity),
            "EmailSubmission" => Some(Object::EmailSubmission),
            "VacationResponse" => Some(Object::VacationResponse),
            "PushSubscription" => Some(Object::PushSubscription),
            _ => None,
        }
    }
}

impl From<Collection> for Object {
    fn from(col: Collection) -> Self {
        match col {
            Collection::Mail => Object::Email,
            Collection::Mailbox => Object::Mailbox,
            Collection::Thread => Object::Thread,
            Collection::Identity => Object::Identity,
            Collection::EmailSubmission => Object::EmailSubmission,
            Collection::VacationResponse => Object::VacationResponse,
            Collection::PushSubscription => Object::PushSubscription,
            Collection::Account | Collection::None => unreachable!(),
        }
    }
}

impl From<Object> for Collection {
    fn from(obj: Object) -> Self {
        match obj {
            Object::Email => Collection::Mail,
            Object::Mailbox => Collection::Mailbox,
            Object::Thread => Collection::Thread,
            Object::Identity => Collection::Identity,
            Object::EmailSubmission => Collection::EmailSubmission,
            Object::VacationResponse => Collection::VacationResponse,
            Object::PushSubscription => Collection::PushSubscription,
            Object::SearchSnippet | Object::Core => Collection::None,
        }
    }
}

impl Invocation {
    pub fn parse(
        name: &str,
        arguments: JSONValue,
        response: &Response,
        config: &JMAPConfig,
    ) -> crate::Result<Self> {
        let mut name_parts = name.split('/');
        let obj = name_parts.next().and_then(Object::parse).ok_or_else(|| {
            MethodError::InvalidArguments(format!("Failed to parse method name: {}.", name))
        })?;

        let (account_id, call) = match name_parts.next().ok_or_else(|| {
            MethodError::InvalidArguments(format!("Failed to parse method name: {}.", name))
        })? {
            "get" => {
                let r = GetRequest::parse(arguments, response)?;
                (r.account_id, Method::Get(r))
            }
            "set" => {
                let r = SetRequest::parse(arguments, response)?;
                if r.create.len() + r.update.len() + r.destroy.len() > config.max_objects_in_set {
                    return Err(MethodError::RequestTooLarge);
                }
                (r.account_id, Method::Set(r))
            }
            "query" => {
                let r = QueryRequest::parse(arguments, response)?;
                (r.account_id, Method::Query(r))
            }
            "queryChanges" => {
                let r = QueryChangesRequest::parse(arguments, response)?;
                (r.account_id, Method::QueryChanges(r))
            }
            "changes" => {
                let r = ChangesRequest::parse(arguments, response)?;
                (r.account_id, Method::Changes(r))
            }
            "import" => {
                let r = ImportRequest::parse(arguments, response)?;
                (r.account_id, Method::Import(r))
            }
            "parse" => {
                let r = ParseRequest::parse(arguments, response)?;
                (r.account_id, Method::Parse(r))
            }
            "echo" => (0, Method::Echo(arguments)),
            _ => {
                return Err(MethodError::UnknownMethod(format!(
                    "Unknown method: {}",
                    name
                )))
            }
        };

        Ok(Invocation {
            obj,
            call,
            account_id,
        })
    }

    pub fn update_set_flags(&mut self, set_tombstone_deletions: bool) -> bool {
        if let Method::Set(SetRequest {
            tombstone_deletions,
            ..
        }) = &mut self.call
        {
            *tombstone_deletions = set_tombstone_deletions;
            true
        } else {
            false
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Core => write!(f, "Core"),
            Object::Mailbox => write!(f, "Mailbox"),
            Object::Thread => write!(f, "Thread"),
            Object::Email => write!(f, "Email"),
            Object::SearchSnippet => write!(f, "SearchSnippet"),
            Object::Identity => write!(f, "Identity"),
            Object::EmailSubmission => write!(f, "EmailSubmission"),
            Object::VacationResponse => write!(f, "VacationResponse"),
            Object::PushSubscription => write!(f, "PushSubscription"),
        }
    }
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Echo(..) => write!(f, "echo"),
            Method::Get(..) => write!(f, "get"),
            Method::Set(..) => write!(f, "set"),
            Method::Query(..) => write!(f, "query"),
            Method::QueryChanges(..) => write!(f, "queryChanges"),
            Method::Changes(..) => write!(f, "changes"),
            Method::Import(..) => write!(f, "import"),
            Method::Parse(..) => write!(f, "parse"),
        }
    }
}

impl Display for Invocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.obj, self.call)
    }
}