use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub(crate) mod codec;

pub struct Http<T: DeserializeOwned> {
    _marker: PhantomData<T>,
}
impl<T: Serialize + DeserializeOwned> Http<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}
