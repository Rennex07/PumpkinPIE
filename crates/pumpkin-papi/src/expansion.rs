//! Expansions registered with the provider.
//!
//! An expansion is a namespace owned by one plugin. Every placeholder it
//! answers is `<namespace>_<name>`, so two plugins cannot collide on an id as
//! long as they pick different namespaces.

use std::collections::{BTreeMap, BTreeSet};

use crate::protocol::{Cache, ProtocolError, check_name, check_namespace};

/// A namespace, the plugin that owns it, and how its values may be cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// The namespace, always lowercase.
    pub namespace: String,
    /// The plugin that claimed it.
    pub source: String,
    /// Names the expansion declared. Empty means it answers whatever it is
    /// asked for, which is what a hook style registration looks like.
    pub names: BTreeSet<String>,
    /// The TTL the provider applies, already clamped.
    pub cache: Cache,
}

impl Expansion {
    /// Whether this expansion declared `name` as one of its placeholders.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.names.is_empty() || self.names.contains(name)
    }

    /// The TTL to cache this expansion's values for, in milliseconds.
    #[must_use]
    pub fn ttl_ms(&self) -> u64 {
        self.cache.effective_ttl_ms()
    }
}

/// The provider's expansion table.
#[derive(Debug, Default)]
pub struct Registry {
    expansions: BTreeMap<String, Expansion>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.expansions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.expansions.is_empty()
    }

    /// Every registered namespace, sorted.
    pub fn namespaces(&self) -> Vec<&str> {
        self.expansions.keys().map(String::as_str).collect()
    }

    /// Every registered expansion, sorted by namespace.
    pub fn all(&self) -> impl Iterator<Item = &Expansion> {
        self.expansions.values()
    }

    #[must_use]
    pub fn get(&self, namespace: &str) -> Option<&Expansion> {
        self.expansions.get(namespace)
    }

    /// Claims or updates a namespace, normalising its case.
    ///
    /// # Errors
    /// Returns [`ProtocolError`] for a malformed or reserved namespace, or a
    /// malformed name.
    pub fn add<I>(
        &mut self,
        namespace: &str,
        source: &str,
        names: I,
        cache: Cache,
    ) -> Result<&Expansion, ProtocolError>
    where
        I: IntoIterator<Item = String>,
    {
        let key = check_namespace(&namespace.to_ascii_lowercase())?.to_string();
        let mut cleaned = BTreeSet::new();
        for name in names {
            let name = name.to_ascii_lowercase();
            check_name(&name)?;
            cleaned.insert(name);
        }

        let expansion = self
            .expansions
            .entry(key.clone())
            .or_insert_with(|| Expansion {
                namespace: key.clone(),
                source: source.to_string(),
                names: BTreeSet::new(),
                cache: Cache::Never,
            });
        expansion.source = source.to_string();
        expansion.names = cleaned;
        // Stored as the clamped TTL rather than the request, so `Cache::Never`
        // would otherwise be indistinguishable from a TTL that clamped to zero.
        expansion.cache = match cache.effective_ttl_ms() {
            0 => Cache::Never,
            ms => Cache::Ttl { ms },
        };

        Ok(self.expansions.get(&key).expect("entry was just inserted"))
    }

    /// Forgets every expansion `source` claimed, returning the namespaces.
    pub fn drop_source(&mut self, source: &str) -> Vec<String> {
        let removed: Vec<String> = self
            .expansions
            .iter()
            .filter(|(_, expansion)| expansion.source == source)
            .map(|(namespace, _)| namespace.clone())
            .collect();
        for namespace in &removed {
            self.expansions.remove(namespace);
        }
        removed
    }

    /// The expansion that owns `identifier`, if any.
    ///
    /// The namespace is everything before the first underscore, so
    /// `luckperms_prefix` belongs to `luckperms`.
    #[must_use]
    pub fn owner_of(&self, identifier: &str) -> Option<&Expansion> {
        let lowered = identifier.to_ascii_lowercase();
        let namespace = lowered.split_once('_')?.0;
        self.expansions.get(namespace)
    }
}
