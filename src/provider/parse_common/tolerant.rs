use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub(crate) fn deserialize_vec_skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };

    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };

    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

pub(crate) fn deserialize_optional_vec_skip_invalid<'de, D, T>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    let Value::Array(items) = value else {
        return Ok(None);
    };

    Ok(Some(
        items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
    ))
}

pub(crate) fn deserialize_optional_struct_skip_invalid<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    Ok(serde_json::from_value(value).ok())
}
