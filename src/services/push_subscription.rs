use super::{state_change::StateChange, LONG_SLUMBER_MS, THROTTLE_MS};
use crate::{api::StateChangeResponse, cluster::IPC_CHANNEL_BUFFER, JMAPServer};
use jmap::{
    base64,
    orm::serialize::JMAPOrm,
    push_subscription::schema::{self, Property, Value},
    types::{jmap::JMAPId, type_state::TypeState},
};
use reqwest::header::{CONTENT_ENCODING, CONTENT_TYPE};
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    time::{Duration, Instant, SystemTime},
};
use store::{
    core::{bitmap::Bitmap, collection::Collection, error::StoreError},
    tracing::debug,
    AccountId, DocumentId, Store,
};
use tokio::{sync::mpsc, time};

#[derive(Debug)]
pub enum UpdateSubscription {
    Unverified {
        id: DocumentId,
        url: String,
        code: String,
        keys: Option<EncriptionKeys>,
    },
    Verified(PushSubscription),
}

#[derive(Debug)]
pub struct PushSubscription {
    pub id: DocumentId,
    pub url: String,
    pub expires: u64,
    pub types: Bitmap<TypeState>,
    pub keys: Option<EncriptionKeys>,
}

#[derive(Debug, Clone)]
pub struct EncriptionKeys {
    pub p256dh: Vec<u8>,
    pub auth: Vec<u8>,
}

#[derive(Debug)]
pub enum Event {
    Update {
        updates: Vec<PushUpdate>,
    },
    Push {
        ids: Vec<store::JMAPId>,
        state_change: StateChange,
    },
    DeliverySuccess {
        id: store::JMAPId,
    },
    DeliveryFailure {
        id: store::JMAPId,
        state_changes: Vec<StateChange>,
    },
    Reset,
}

#[derive(Debug)]
pub enum PushUpdate {
    Verify {
        id: DocumentId,
        account_id: AccountId,
        url: String,
        code: String,
        keys: Option<EncriptionKeys>,
    },
    Register {
        id: store::JMAPId,
        url: String,
        keys: Option<EncriptionKeys>,
    },
    Unregister {
        id: store::JMAPId,
    },
}

#[derive(Debug)]
pub struct PushServer {
    url: String,
    keys: Option<EncriptionKeys>,
    num_attempts: u32,
    last_request: Instant,
    state_changes: Vec<StateChange>,
    in_flight: bool,
}

#[cfg(test)]
const PUSH_ATTEMPT_INTERVAL_MS: u64 = 500;
#[cfg(not(test))]
const PUSH_ATTEMPT_INTERVAL_MS: u64 = 60 * 1000;
const PUSH_MAX_ATTEMPTS: u32 = 3;
const PUSH_TIMEOUT_MS: u64 = 10 * 1000;
const RETRY_MS: u64 = 1000;
const VERIFY_WAIT_MS: u64 = 60 * 1000;

pub fn spawn_push_manager() -> mpsc::Sender<Event> {
    let (push_tx_, mut push_rx) = mpsc::channel::<Event>(IPC_CHANNEL_BUFFER);
    let push_tx = push_tx_.clone();

    tokio::spawn(async move {
        let mut subscriptions = HashMap::new();
        let mut last_verify: HashMap<AccountId, u64> = HashMap::new();
        let mut last_retry = Instant::now();
        let mut retry_timeout = Duration::from_millis(LONG_SLUMBER_MS);
        let mut retry_ids = HashSet::new();

        loop {
            match time::timeout(retry_timeout, push_rx.recv()).await {
                Ok(Some(event)) => {
                    //println!("Push: {:?}", event);

                    match event {
                        Event::Update { updates } => {
                            for update in updates {
                                match update {
                                    PushUpdate::Verify {
                                        id,
                                        account_id,
                                        url,
                                        code,
                                        keys,
                                    } => {
                                        let current_time = SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);

                                        #[cfg(test)]
                                        if url.contains("skip_checks") {
                                            last_verify.insert(
                                                account_id,
                                                current_time - (VERIFY_WAIT_MS + 1),
                                            );
                                        }

                                        if last_verify
                                            .get(&account_id)
                                            .map(|last_verify| {
                                                current_time - *last_verify > VERIFY_WAIT_MS
                                            })
                                            .unwrap_or(true)
                                        {
                                            tokio::spawn(async move {
                                                http_request(
                                                    url,
                                                    format!(
                                                        concat!(
                                                            "{{\"@type\":\"PushVerification\",",
                                                            "\"pushSubscriptionId\":\"{}\",",
                                                            "\"verificationCode\":\"{}\"}}"
                                                        ),
                                                        JMAPId::from(id),
                                                        code
                                                    ),
                                                    keys,
                                                )
                                                .await;
                                            });

                                            last_verify.insert(account_id, current_time);
                                        } else {
                                            debug!(
                                                concat!(
                                                    "Failed to verify push subscription: ",
                                                    "Too many requests for from accountId {}."
                                                ),
                                                account_id
                                            );
                                            continue;
                                        }
                                    }
                                    PushUpdate::Register { id, url, keys } => {
                                        if let Entry::Vacant(entry) = subscriptions.entry(id) {
                                            entry.insert(PushServer {
                                                url,
                                                keys,
                                                num_attempts: 0,
                                                last_request: Instant::now()
                                                    - Duration::from_millis(THROTTLE_MS + 1),
                                                state_changes: Vec::new(),
                                                in_flight: false,
                                            });
                                        }
                                    }
                                    PushUpdate::Unregister { id } => {
                                        subscriptions.remove(&id);
                                    }
                                }
                            }
                        }
                        Event::Push { ids, state_change } => {
                            for id in ids {
                                if let Some(subscription) = subscriptions.get_mut(&id) {
                                    subscription.state_changes.push(state_change.clone());
                                    let last_request =
                                        subscription.last_request.elapsed().as_millis() as u64;

                                    if !subscription.in_flight
                                        && ((subscription.num_attempts == 0
                                            && last_request > THROTTLE_MS)
                                            || ((1..PUSH_MAX_ATTEMPTS)
                                                .contains(&subscription.num_attempts)
                                                && last_request > PUSH_ATTEMPT_INTERVAL_MS))
                                    {
                                        subscription.send(id, push_tx.clone());
                                        retry_ids.remove(&id);
                                    } else {
                                        retry_ids.insert(id);
                                    }
                                } else {
                                    debug!("No push subscription found for id: {}", id);
                                }
                            }
                        }
                        Event::Reset => {
                            subscriptions.clear();
                        }
                        Event::DeliverySuccess { id } => {
                            if let Some(subscription) = subscriptions.get_mut(&id) {
                                subscription.num_attempts = 0;
                                subscription.in_flight = false;
                                retry_ids.remove(&id);
                            }
                        }
                        Event::DeliveryFailure { id, state_changes } => {
                            if let Some(subscription) = subscriptions.get_mut(&id) {
                                subscription.last_request = Instant::now();
                                subscription.num_attempts += 1;
                                subscription.state_changes.extend(state_changes);
                                subscription.in_flight = false;
                                retry_ids.insert(id);
                            }
                        }
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(_) => (),
            }

            retry_timeout = if !retry_ids.is_empty() {
                let last_retry_elapsed = last_retry.elapsed().as_millis() as u64;

                if last_retry_elapsed >= RETRY_MS {
                    let mut remove_ids = Vec::with_capacity(retry_ids.len());

                    for retry_id in &retry_ids {
                        if let Some(subscription) = subscriptions.get_mut(retry_id) {
                            let last_request =
                                subscription.last_request.elapsed().as_millis() as u64;

                            if !subscription.in_flight
                                && ((subscription.num_attempts == 0 && last_request >= THROTTLE_MS)
                                    || (subscription.num_attempts > 0
                                        && last_request >= PUSH_ATTEMPT_INTERVAL_MS))
                            {
                                if subscription.num_attempts < PUSH_MAX_ATTEMPTS {
                                    subscription.send(*retry_id, push_tx.clone());
                                } else {
                                    debug!(
                                        concat!(
                                            "Failed to deliver push subscription: ",
                                            "Too many attempts for url {}."
                                        ),
                                        subscription.url
                                    );
                                    subscription.state_changes.clear();
                                    subscription.num_attempts = 0;
                                }
                                remove_ids.push(*retry_id);
                            }
                        } else {
                            remove_ids.push(*retry_id);
                        }
                    }

                    if remove_ids.len() < retry_ids.len() {
                        for remove_id in remove_ids {
                            retry_ids.remove(&remove_id);
                        }
                        last_retry = Instant::now();
                        Duration::from_millis(RETRY_MS)
                    } else {
                        retry_ids.clear();
                        Duration::from_millis(LONG_SLUMBER_MS)
                    }
                } else {
                    Duration::from_millis(RETRY_MS - last_retry_elapsed)
                }
            } else {
                Duration::from_millis(LONG_SLUMBER_MS)
            };
            //println!("Retry ids {:?} in {:?}", retry_ids, retry_timeout);
        }
    });

    push_tx_
}

impl PushServer {
    fn send(&mut self, id: store::JMAPId, push_tx: mpsc::Sender<Event>) {
        let url = self.url.clone();
        let keys = self.keys.clone();
        let state_changes = std::mem::take(&mut self.state_changes);

        self.in_flight = true;
        self.last_request = Instant::now();

        tokio::spawn(async move {
            let mut response = StateChangeResponse::new();
            for state_change in &state_changes {
                for (type_state, change_id) in &state_change.types {
                    response
                        .changed
                        .entry(state_change.account_id.into())
                        .or_insert_with(HashMap::new)
                        .insert(type_state.clone(), (*change_id).into());
                }
            }

            //println!("Posting to {}: {:?}", url, response);

            push_tx
                .send(
                    if http_request(url, serde_json::to_string(&response).unwrap(), keys).await {
                        Event::DeliverySuccess { id }
                    } else {
                        Event::DeliveryFailure { id, state_changes }
                    },
                )
                .await
                .ok();
        });
    }
}

async fn http_request(url: String, mut body: String, keys: Option<EncriptionKeys>) -> bool {
    let client_builder = reqwest::Client::builder().timeout(Duration::from_millis(PUSH_TIMEOUT_MS));

    #[cfg(test)]
    let client_builder = client_builder.danger_accept_invalid_certs(true);

    let mut client = client_builder
        .build()
        .unwrap_or_default()
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header("TTL", "86400");

    if let Some(keys) = keys {
        match ece::encrypt(&keys.p256dh, &keys.auth, body.as_bytes())
            .map(|b| base64::encode_config(b, base64::URL_SAFE))
        {
            Ok(body_) => {
                body = body_;
                client = client.header(CONTENT_ENCODING, "aes128gcm");
            }
            Err(err) => {
                // Do not reattempt if encryption fails.
                debug!("Failed to encrypt push subscription to {}: {}", url, err);
                return true;
            }
        }
    }

    match client.body(body).send().await {
        Ok(response) => response.status().is_success(),
        Err(err) => {
            debug!("HTTP post to {} failed with: {}", url, err);
            false
        }
    }
}

impl<T> JMAPServer<T>
where
    T: for<'x> Store<'x> + 'static,
{
    pub async fn fetch_push_subscriptions(
        &self,
        account_id: AccountId,
    ) -> jmap::Result<super::state_change::Event> {
        let store = self.store.clone();

        self.spawn_jmap_request(move || {
            let mut subscriptions = Vec::new();
            let document_ids = store
                .get_document_ids(account_id, Collection::PushSubscription)?
                .unwrap_or_default();
            let current_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            for document_id in document_ids {
                let mut subscription = store
                    .get_orm::<schema::PushSubscription>(account_id, document_id)?
                    .ok_or_else(|| {
                        StoreError::InternalError(format!(
                            "Could not find ORM for push subscription {}",
                            document_id
                        ))
                    })?;
                let expires = subscription
                    .get(&Property::Expires)
                    .and_then(|p| p.as_timestamp())
                    .ok_or_else(|| {
                        StoreError::InternalError(format!(
                            "Missing expires property for push subscription {}",
                            document_id
                        ))
                    })? as u64;
                if expires > current_time {
                    let keys =
                        if let Some(Value::Keys { value }) = subscription.remove(&Property::Keys) {
                            EncriptionKeys {
                                p256dh: base64::decode_config(&value.p256dh, base64::URL_SAFE)
                                    .unwrap_or_default(),
                                auth: base64::decode_config(&value.auth, base64::URL_SAFE)
                                    .unwrap_or_default(),
                            }
                            .into()
                        } else {
                            None
                        };
                    let verification_code = subscription
                        .remove(&Property::VerificationCode_)
                        .and_then(|p| p.unwrap_text())
                        .ok_or_else(|| {
                            StoreError::InternalError(format!(
                                "Missing verificationCode property for push subscription {}",
                                document_id
                            ))
                        })?;
                    let url = subscription
                        .remove(&Property::Url)
                        .and_then(|p| p.unwrap_text())
                        .ok_or_else(|| {
                            StoreError::InternalError(format!(
                                "Missing Url property for push subscription {}",
                                document_id
                            ))
                        })?;

                    if subscription
                        .get(&Property::VerificationCode)
                        .and_then(|p| p.as_text())
                        .map_or(false, |v| v == verification_code)
                    {
                        let types = if let Some(Value::Types { value }) =
                            subscription.remove(&Property::Types)
                        {
                            if !value.is_empty() {
                                value.into()
                            } else {
                                Bitmap::all()
                            }
                        } else {
                            Bitmap::all()
                        };

                        // Add verified subscription
                        subscriptions.push(UpdateSubscription::Verified(PushSubscription {
                            id: document_id,
                            url,
                            expires,
                            types,
                            keys,
                        }));
                    } else {
                        // Add unverified subscription
                        subscriptions.push(UpdateSubscription::Unverified {
                            id: document_id,
                            url,
                            code: verification_code,
                            keys,
                        });
                    }
                }
            }

            Ok(super::state_change::Event::UpdateSubscriptions {
                account_id,
                subscriptions,
            })
        })
        .await
    }
}