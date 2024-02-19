use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use sha2::Digest as Sha2Digest;
use sha2::{self, Sha256};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Digest(pub digest::Output<sha2::Sha256>);

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DigestVisitor;
        impl<'de> Visitor<'de> for DigestVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sha256 digest")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Vec::from(v))
            }
        }

        let inner = deserializer.deserialize_bytes(DigestVisitor)?;
        let inner = digest::Output::<Sha256>::from_slice(inner.as_slice());

        Ok(Digest::from(*inner))
    }
}

impl Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl std::fmt::Debug for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("sha256:")?;
        for b in self.0.iter() {
            f.write_fmt(format_args!("{:02x}", b))?;
        }
        Ok(())
    }
}

pub fn zero_digest() -> Digest {
    Digest(*digest::Output::<sha2::Sha256>::from_slice(
        &vec![0u8; sha2::Sha256::output_size()],
    ))
}

impl From<digest::Output<sha2::Sha256>> for Digest {
    fn from(d: digest::Output<sha2::Sha256>) -> Self {
        Self(d)
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub fn hash_one_thing<T1>(label1: &str, v1: T1) -> Digest
where
    T1: AsRef<[u8]>,
{
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"hash_one_thing");
    {
        hasher.update(label1.as_bytes());
        let r1: &[u8] = v1.as_ref();
        hasher.update(r1.len().to_le_bytes());
        hasher.update(r1);
    }

    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bincode_serializer() {
        let digest = hash_one_thing("Test Value", "Hey there!");
        let serialized = bincode::serialize(&digest).unwrap();
        let deserialized: Digest = bincode::deserialize(&serialized).unwrap();
        assert_eq!(digest, deserialized);
    }
}
