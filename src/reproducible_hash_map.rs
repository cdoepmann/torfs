use twox_hash::XxHash64;

/// A reproducible HashMap
pub type RHashMap<K, V> = std::collections::HashMap<K, V, std::hash::BuildHasherDefault<XxHash64>>;

/// A reproducible HashSet
pub type RHashSet<K> = std::collections::HashSet<K, std::hash::BuildHasherDefault<XxHash64>>;
