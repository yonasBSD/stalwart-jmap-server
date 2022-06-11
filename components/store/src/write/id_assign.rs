use std::sync::Arc;

use parking_lot::Mutex;
use roaring::RoaringBitmap;

use crate::{
    serialize::key::BitmapKey, AccountId, Collection, DocumentId, JMAPStore, Store, StoreError,
};

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct IdCacheKey {
    pub account_id: AccountId,
    pub collection: Collection,
}

impl IdCacheKey {
    pub fn new(account_id: AccountId, collection: Collection) -> Self {
        Self {
            account_id,
            collection,
        }
    }
}

#[derive(Clone)]
pub struct IdAssigner {
    pub freed_ids: Option<RoaringBitmap>,
    pub next_id: DocumentId,
}

impl IdAssigner {
    pub fn new(used_ids: Option<RoaringBitmap>) -> Self {
        let (next_id, freed_ids) = if let Some(used_ids) = used_ids {
            let next_id = used_ids.max().unwrap() + 1;
            //TODO test properly
            let mut freed_ids = RoaringBitmap::from_sorted_iter(0..next_id).unwrap();
            freed_ids ^= used_ids;
            (
                next_id,
                if !freed_ids.is_empty() {
                    Some(freed_ids)
                } else {
                    None
                },
            )
        } else {
            (0, None)
        };
        Self { freed_ids, next_id }
    }

    pub fn assign_document_id(&mut self) -> DocumentId {
        if let Some(freed_ids) = &mut self.freed_ids {
            let id = freed_ids.min().unwrap();
            freed_ids.remove(id);
            if freed_ids.is_empty() {
                self.freed_ids = None;
            }
            id
        } else {
            let id = self.next_id;
            self.next_id += 1;
            id
        }
    }
}

impl<T> JMAPStore<T>
where
    T: for<'x> Store<'x> + 'static,
{
    pub fn get_id_assigner(
        &self,
        account_id: AccountId,
        collection: Collection,
    ) -> crate::Result<Arc<Mutex<IdAssigner>>> {
        self.id_assigner
            .try_get_with::<_, StoreError>(IdCacheKey::new(account_id, collection), || {
                Ok(Arc::new(Mutex::new(IdAssigner::new(
                    self.get_document_ids(account_id, collection)?,
                ))))
            })
            .map_err(|e| e.as_ref().clone())
    }

    pub fn assign_document_id(
        &self,
        account_id: AccountId,
        collection: Collection,
    ) -> crate::Result<DocumentId> {
        Ok(self
            .get_id_assigner(account_id, collection)?
            .lock()
            .assign_document_id())
    }

    pub fn get_document_ids(
        &self,
        account_id: AccountId,
        collection: Collection,
    ) -> crate::Result<Option<RoaringBitmap>> {
        self.get_bitmap(&BitmapKey::serialize_document_ids(account_id, collection))
    }
}

//TODO test

/*#[cfg(test)]
pub fn set_document_ids(
    &self,
    account_id: AccountId,
    collection: Collection,
    bitmap: RoaringBitmap,
) -> crate::Result<()> {
    use crate::bitmaps::IS_BITMAP;

    let mut bytes = Vec::with_capacity(bitmap.serialized_size() + 1);
    bytes.push(IS_BITMAP);
    bitmap
        .serialize_into(&mut bytes)
        .map_err(|e| StoreError::InternalError(e.to_string()))?;

    self.db
        .put_cf(
            &self.get_handle("bitmaps")?,
            &serialize_bm_internal(account_id, collection, BM_USED_IDS),
            bytes,
        )
        .map_err(|e| StoreError::InternalError(e.to_string()))
}*/
/*
#[cfg(test)]
mod tests {
    use std::{ops::BitXorAssign, sync::Arc, thread};

    use roaring::RoaringBitmap;

    use crate::RocksDBStore;

    #[test]
    fn id_assigner() {
        rayon::ThreadPoolBuilder::new()
            .num_threads(20)
            .build()
            .unwrap()
            .scope(|s| {
                let mut temp_dir = std::env::temp_dir();
                temp_dir.push("strdb_id_test");
                if temp_dir.exists() {
                    std::fs::remove_dir_all(&temp_dir).unwrap();
                }

                let db = Arc::new(RocksDBStore::open(temp_dir.to_str().unwrap()).unwrap());

                for _ in 0..10 {
                    let db = db.clone();
                    s.spawn(move |_| {
                        let mut uncommited_ids = Vec::new();
                        for _ in 0..100 {
                            uncommited_ids.push(db.get_next_document_id(0, 0).unwrap());
                        }
                        thread::sleep(std::time::Duration::from_millis(100));
                    });
                }
                thread::sleep(std::time::Duration::from_millis(200));

                // Uncommitted ids should be released
                assert_eq!(
                    db.remove_id_assigner(0, 0).unwrap().get_available_ids(),
                    &(0..1000).collect::<RoaringBitmap>()
                );

                // Deleted ids should be made available
                let mut used_ids = RoaringBitmap::new();
                let mut x = (1, 1);
                for _ in 0..10 {
                    used_ids.insert(x.0);
                    x = (x.1, x.0 + x.1)
                }
                for i in 56..=60 {
                    used_ids.insert(i);
                }
                let mut expected_ids = (0..=60).collect::<RoaringBitmap>();
                expected_ids.bitxor_assign(&used_ids);
                expected_ids.insert_range(61..=63);
                db.set_document_ids(0, 0, used_ids).unwrap();

                for _ in 0..50 {
                    let mut doc_id = db.get_next_document_id(0, 0).unwrap();
                    assert!(
                        expected_ids.contains(doc_id.get_id()),
                        "Unexpected id {}",
                        doc_id.get_id()
                    );
                    expected_ids.remove(doc_id.get_id());
                    doc_id.commit();
                }

                assert!(expected_ids.is_empty(), "Missing ids: {:?}", expected_ids);

                std::fs::remove_dir_all(&temp_dir).unwrap();
            });
    }
}*/
