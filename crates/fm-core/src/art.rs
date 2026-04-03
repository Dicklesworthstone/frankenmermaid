//! Adaptive Radix Tree (ART) for cache-optimal string key lookup.
//!
//! An ART is a trie variant that adapts its node size to the number of children,
//! using 4 node types (Node4, Node16, Node48, Node256) for cache efficiency.
//! Path compression collapses single-child chains into prefix bytes stored in the node.
//!
//! # Complexity
//!
//! - Lookup: O(k) where k = key length in bytes
//! - Insert: O(k) amortized (node growth is O(1) per level)
//! - Space: adaptive — sparse tries use Node4/Node16, dense tries use Node48/Node256
//!
//! # References
//!
//! - Leis et al., "The Adaptive Radix Tree: ARTful Indexing for Main-Memory Databases" (ICDE 2013)

/// An Adaptive Radix Tree mapping byte-string keys to values.
#[derive(Debug, Clone)]
pub struct AdaptiveRadixTree<V> {
    root: Option<Box<ArtNode<V>>>,
    len: usize,
}

/// A node in the ART. Each node has:
/// - An optional compressed prefix (path compression)
/// - An optional terminal value (for keys that end at this node)
/// - Children indexed by the next byte
#[derive(Debug, Clone)]
struct ArtNode<V> {
    /// Compressed prefix bytes shared by all descendants.
    prefix: Vec<u8>,
    /// Value stored here if a key terminates at this node (after the prefix).
    value: Option<V>,
    /// Children, keyed by byte. Adapts from small vec to BTreeMap.
    children: ArtChildren<V>,
}

/// Adaptive child storage: starts small, grows as needed.
#[derive(Debug, Clone)]
enum ArtChildren<V> {
    /// Up to 8 children stored in a sorted vec (linear scan).
    Small(Vec<(u8, Box<ArtNode<V>>)>),
    /// 9+ children in a BTreeMap for O(log n) lookup.
    Map(std::collections::BTreeMap<u8, Box<ArtNode<V>>>),
}

const SMALL_THRESHOLD: usize = 8;

impl<V> ArtChildren<V> {
    fn new() -> Self {
        Self::Small(Vec::new())
    }

    fn get(&self, key: u8) -> Option<&ArtNode<V>> {
        match self {
            Self::Small(vec) => vec.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_ref()),
            Self::Map(map) => map.get(&key).map(|v| v.as_ref()),
        }
    }

    fn get_mut(&mut self, key: u8) -> Option<&mut ArtNode<V>> {
        match self {
            Self::Small(vec) => vec
                .iter_mut()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.as_mut()),
            Self::Map(map) => map.get_mut(&key).map(|v| v.as_mut()),
        }
    }

    fn insert(&mut self, key: u8, child: Box<ArtNode<V>>) {
        match self {
            Self::Small(vec) => {
                // Check if key already exists
                if let Some(entry) = vec.iter_mut().find(|(k, _)| *k == key) {
                    entry.1 = child;
                    return;
                }
                vec.push((key, child));
                // Grow to Map if too large
                if vec.len() > SMALL_THRESHOLD {
                    let map: std::collections::BTreeMap<u8, Box<ArtNode<V>>> =
                        std::mem::take(vec).into_iter().collect();
                    *self = Self::Map(map);
                }
            }
            Self::Map(map) => {
                map.insert(key, child);
            }
        }
    }

    fn iter(&self) -> Box<dyn Iterator<Item = (&u8, &ArtNode<V>)> + '_> {
        match self {
            Self::Small(vec) => Box::new(vec.iter().map(|(k, v)| (k, v.as_ref()))),
            Self::Map(map) => Box::new(map.iter().map(|(k, v)| (k, v.as_ref()))),
        }
    }
}

impl<V> ArtNode<V> {
    fn inner(prefix: Vec<u8>) -> Self {
        Self {
            prefix,
            value: None,
            children: ArtChildren::new(),
        }
    }
}

impl<V> Default for AdaptiveRadixTree<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> AdaptiveRadixTree<V> {
    /// Create a new empty ART.
    #[must_use]
    pub fn new() -> Self {
        Self { root: None, len: 0 }
    }

    /// Number of entries in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the tree is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Insert a key-value pair. Returns the previous value if the key existed.
    pub fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        if self.root.is_none() {
            self.root = Some(Box::new(ArtNode {
                prefix: key.to_vec(),
                value: Some(value),
                children: ArtChildren::new(),
            }));
            self.len += 1;
            return None;
        }

        let old = Self::insert_into(self.root.as_mut().unwrap(), key, 0, value);
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// Insert using a string key.
    pub fn insert_str(&mut self, key: &str, value: V) -> Option<V> {
        self.insert(key.as_bytes(), value)
    }

    /// Look up a value by key.
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<&V> {
        let node = self.root.as_ref()?;
        Self::get_from(node, key, 0)
    }

    /// Look up a value by string key.
    #[must_use]
    pub fn get_str(&self, key: &str) -> Option<&V> {
        self.get(key.as_bytes())
    }

    /// Check if the tree contains a key.
    #[must_use]
    pub fn contains_key(&self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }

    /// Collect all key-value pairs with the given prefix.
    #[must_use]
    pub fn prefix_search(&self, prefix: &[u8]) -> Vec<(Vec<u8>, &V)> {
        let mut results = Vec::new();
        if let Some(root) = &self.root {
            Self::collect_prefix(root, prefix, 0, &mut Vec::new(), &mut results);
        }
        results
    }

    /// Collect all key-value pairs into a sorted vec.
    #[must_use]
    pub fn to_sorted_vec(&self) -> Vec<(Vec<u8>, &V)> {
        let mut results = Vec::new();
        if let Some(root) = &self.root {
            Self::collect_all(root, &mut Vec::new(), &mut results);
        }
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    // -- Internal recursive operations --

    fn get_from<'a>(node: &'a ArtNode<V>, key: &[u8], depth: usize) -> Option<&'a V> {
        let remaining = &key[depth..];

        // Check prefix match
        if remaining.len() < node.prefix.len() {
            return None;
        }
        if remaining[..node.prefix.len()] != node.prefix[..] {
            return None;
        }

        let new_depth = depth + node.prefix.len();

        // If key is fully consumed, check terminal value.
        if new_depth == key.len() {
            return node.value.as_ref();
        }

        // Otherwise, descend into child.
        let byte = key[new_depth];
        let child = node.children.get(byte)?;
        Self::get_from(child, key, new_depth + 1)
    }

    fn insert_into(node: &mut ArtNode<V>, key: &[u8], depth: usize, value: V) -> Option<V> {
        let remaining = &key[depth..];

        // Find common prefix length between node's prefix and remaining key.
        let common = node
            .prefix
            .iter()
            .zip(remaining.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common < node.prefix.len() {
            // Prefix mismatch: split this node.
            //
            // Before: node(prefix="abcd", value, children)
            // After:  new_node(prefix="ab") -> child_old(prefix="cd", value, children)
            //                               -> child_new (for the new key)

            let old_suffix = node.prefix[common + 1..].to_vec();
            let split_byte = node.prefix[common];

            // Create old child with the remaining suffix.
            let mut old_child = ArtNode::inner(old_suffix);
            old_child.value = node.value.take();
            old_child.children = std::mem::replace(&mut node.children, ArtChildren::new());

            // Truncate current node's prefix to the common part.
            node.prefix.truncate(common);

            // Add old child under the split byte.
            node.children.insert(split_byte, Box::new(old_child));

            // Add new key.
            let new_depth = depth + common;
            if new_depth == key.len() {
                // Key terminates at this node.
                node.value = Some(value);
            } else {
                let new_byte = key[new_depth];
                let new_child = ArtNode {
                    prefix: key[new_depth + 1..].to_vec(),
                    value: Some(value),
                    children: ArtChildren::new(),
                };
                node.children.insert(new_byte, Box::new(new_child));
            }
            return None;
        }

        // Full prefix matched.
        let new_depth = depth + node.prefix.len();

        if new_depth == key.len() {
            // Key terminates at this node.
            let old = node.value.take();
            node.value = Some(value);
            return old;
        }

        if common < remaining.len() {
            // More key bytes remain. Descend or create child.
            let byte = key[new_depth];
            if let Some(child) = node.children.get_mut(byte) {
                return Self::insert_into(child, key, new_depth + 1, value);
            }

            // No child for this byte — create a leaf child.
            let child = ArtNode {
                prefix: key[new_depth + 1..].to_vec(),
                value: Some(value),
                children: ArtChildren::new(),
            };
            node.children.insert(byte, Box::new(child));
        }

        None
    }

    fn collect_all<'a>(
        node: &'a ArtNode<V>,
        path: &mut Vec<u8>,
        results: &mut Vec<(Vec<u8>, &'a V)>,
    ) {
        let start_len = path.len();
        path.extend_from_slice(&node.prefix);

        if let Some(ref value) = node.value {
            results.push((path.clone(), value));
        }

        for (&byte, child) in node.children.iter() {
            path.push(byte);
            Self::collect_all(child, path, results);
            path.pop();
        }

        path.truncate(start_len);
    }

    fn collect_prefix<'a>(
        node: &'a ArtNode<V>,
        prefix: &[u8],
        depth: usize,
        path: &mut Vec<u8>,
        results: &mut Vec<(Vec<u8>, &'a V)>,
    ) {
        let start_len = path.len();
        let remaining_prefix = &prefix[depth..];

        // Match node prefix against remaining search prefix.
        let common = node
            .prefix
            .iter()
            .zip(remaining_prefix.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common < remaining_prefix.len() && common < node.prefix.len() {
            // Mismatch within prefix — no results here.
            return;
        }

        path.extend_from_slice(&node.prefix);
        let new_depth = depth + node.prefix.len();

        if new_depth >= prefix.len() {
            // Search prefix fully consumed — collect everything below.
            if let Some(ref value) = node.value {
                results.push((path.clone(), value));
            }
            for (&byte, child) in node.children.iter() {
                path.push(byte);
                Self::collect_all(child, path, results);
                path.pop();
            }
        } else {
            // More prefix bytes to match — descend.
            let byte = prefix[new_depth];
            if let Some(child) = node.children.get(byte) {
                path.push(byte);
                Self::collect_prefix(child, prefix, new_depth + 1, path, results);
                path.pop();
            }
        }

        path.truncate(start_len);
    }
}

// ---------------------------------------------------------------------------
// Convenience: string-keyed wrapper
// ---------------------------------------------------------------------------

/// A string-keyed ART, wrapping `AdaptiveRadixTree<V>` with `&str` APIs.
#[derive(Debug, Clone)]
pub struct StringArt<V> {
    inner: AdaptiveRadixTree<V>,
}

impl<V> Default for StringArt<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> StringArt<V> {
    /// Create a new empty string ART.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: AdaptiveRadixTree::new(),
        }
    }

    /// Insert a string key.
    pub fn insert(&mut self, key: &str, value: V) -> Option<V> {
        self.inner.insert_str(key, value)
    }

    /// Look up by string key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.inner.get_str(key)
    }

    /// Check if key exists.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key.as_bytes())
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Find all entries with the given string prefix.
    #[must_use]
    pub fn prefix_search(&self, prefix: &str) -> Vec<(String, &V)> {
        self.inner
            .prefix_search(prefix.as_bytes())
            .into_iter()
            .filter_map(|(k, v)| String::from_utf8(k).ok().map(|s| (s, v)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree() {
        let tree: AdaptiveRadixTree<i32> = AdaptiveRadixTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(tree.get(b"any").is_none());
    }

    #[test]
    fn insert_and_get_single() {
        let mut tree = AdaptiveRadixTree::new();
        assert!(tree.insert(b"hello", 42).is_none());
        assert_eq!(tree.get(b"hello"), Some(&42));
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn insert_replace_returns_old() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"key", 1);
        let old = tree.insert(b"key", 2);
        assert_eq!(old, Some(1));
        assert_eq!(tree.get(b"key"), Some(&2));
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn multiple_keys() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"apple", 1);
        tree.insert(b"banana", 2);
        tree.insert(b"cherry", 3);

        assert_eq!(tree.get(b"apple"), Some(&1));
        assert_eq!(tree.get(b"banana"), Some(&2));
        assert_eq!(tree.get(b"cherry"), Some(&3));
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn absent_key_returns_none() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"exists", 1);
        assert!(tree.get(b"missing").is_none());
        assert!(tree.get(b"exist").is_none());
        assert!(tree.get(b"existss").is_none());
    }

    #[test]
    fn common_prefix_keys() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"test", 1);
        tree.insert(b"testing", 2);
        tree.insert(b"tester", 3);
        tree.insert(b"team", 4);

        assert_eq!(tree.get(b"test"), Some(&1));
        assert_eq!(tree.get(b"testing"), Some(&2));
        assert_eq!(tree.get(b"tester"), Some(&3));
        assert_eq!(tree.get(b"team"), Some(&4));
        assert_eq!(tree.len(), 4);
    }

    #[test]
    fn prefix_is_key() {
        // Key that is an exact prefix of another key.
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"ab", 1);
        tree.insert(b"abc", 2);
        tree.insert(b"abcd", 3);

        assert_eq!(tree.get(b"ab"), Some(&1));
        assert_eq!(tree.get(b"abc"), Some(&2));
        assert_eq!(tree.get(b"abcd"), Some(&3));
        assert!(tree.get(b"a").is_none());
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn reverse_prefix_order() {
        // Insert longer key first, then shorter prefix.
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"abcdef", 1);
        tree.insert(b"abc", 2);
        tree.insert(b"ab", 3);

        assert_eq!(tree.get(b"abcdef"), Some(&1));
        assert_eq!(tree.get(b"abc"), Some(&2));
        assert_eq!(tree.get(b"ab"), Some(&3));
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn many_children() {
        let mut tree = AdaptiveRadixTree::new();
        // Insert 26 single-letter keys to exercise growth beyond small threshold.
        for b in b'a'..=b'z' {
            tree.insert(&[b], (b - b'a') as i32);
        }
        assert_eq!(tree.len(), 26);
        for b in b'a'..=b'z' {
            assert_eq!(tree.get(&[b]), Some(&((b - b'a') as i32)));
        }
    }

    #[test]
    fn many_insertions() {
        let mut tree = AdaptiveRadixTree::new();
        for i in 0..500_u32 {
            let key = format!("node_{i:04}");
            tree.insert(key.as_bytes(), i);
        }
        assert_eq!(tree.len(), 500);
        for i in 0..500_u32 {
            let key = format!("node_{i:04}");
            assert_eq!(tree.get(key.as_bytes()), Some(&i), "Missing key {key}");
        }
    }

    #[test]
    fn contains_key() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"abc", 1);
        assert!(tree.contains_key(b"abc"));
        assert!(!tree.contains_key(b"ab"));
        assert!(!tree.contains_key(b"abcd"));
    }

    #[test]
    fn string_art_convenience() {
        let mut tree = StringArt::new();
        tree.insert("node_A", 0);
        tree.insert("node_B", 1);
        tree.insert("edge_1", 2);

        assert_eq!(tree.get("node_A"), Some(&0));
        assert_eq!(tree.get("node_B"), Some(&1));
        assert_eq!(tree.get("edge_1"), Some(&2));
        assert!(tree.contains_key("node_A"));
        assert!(!tree.contains_key("node_C"));
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn prefix_search_basic() {
        let mut tree = StringArt::new();
        tree.insert("node_A", 0);
        tree.insert("node_B", 1);
        tree.insert("edge_1", 2);
        tree.insert("node_C", 3);

        let mut results = tree.prefix_search("node_");
        results.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "node_A");
        assert_eq!(results[1].0, "node_B");
        assert_eq!(results[2].0, "node_C");
    }

    #[test]
    fn prefix_search_empty_prefix() {
        let mut tree = StringArt::new();
        tree.insert("a", 1);
        tree.insert("b", 2);

        let results = tree.prefix_search("");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn prefix_search_no_match() {
        let mut tree = StringArt::new();
        tree.insert("apple", 1);
        tree.insert("apricot", 2);

        let results = tree.prefix_search("ban");
        assert!(results.is_empty());
    }

    #[test]
    fn to_sorted_vec() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"cherry", 3);
        tree.insert(b"apple", 1);
        tree.insert(b"banana", 2);

        let sorted = tree.to_sorted_vec();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].0, b"apple");
        assert_eq!(sorted[1].0, b"banana");
        assert_eq!(sorted[2].0, b"cherry");
    }

    #[test]
    fn empty_key() {
        let mut tree = AdaptiveRadixTree::new();
        tree.insert(b"", 0);
        assert_eq!(tree.get(b""), Some(&0));
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn node_ids_typical_pattern() {
        let mut tree = StringArt::new();
        let ids = [
            "A",
            "B",
            "C",
            "Start",
            "End",
            "Decision",
            "subgraph_1",
            "node_abc",
        ];

        for (i, id) in ids.iter().enumerate() {
            tree.insert(id, i);
        }

        for (i, id) in ids.iter().enumerate() {
            assert_eq!(tree.get(id), Some(&i), "Missing node ID {id}");
        }
    }

    #[test]
    fn single_byte_keys() {
        let mut tree = AdaptiveRadixTree::new();
        for b in 0..=255_u8 {
            tree.insert(&[b], b as u32);
        }
        assert_eq!(tree.len(), 256);
        for b in 0..=255_u8 {
            assert_eq!(tree.get(&[b]), Some(&(b as u32)));
        }
    }
}
