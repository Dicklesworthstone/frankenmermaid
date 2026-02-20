//! Type-safe SVG attribute system.
//!
//! Provides a flexible way to manage SVG attributes with proper escaping.

use std::fmt::{self, Write};

/// A single SVG attribute.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: AttributeValue,
}

/// Value of an SVG attribute.
#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Number(f32),
    Integer(i32),
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{}", escape_xml_attr(s)),
            Self::Number(n) => {
                // Format with reasonable precision, trim trailing zeros
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i32)
                } else {
                    write!(f, "{:.2}", n)
                }
            }
            Self::Integer(i) => write!(f, "{i}"),
        }
    }
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<f32> for AttributeValue {
    fn from(n: f32) -> Self {
        Self::Number(n)
    }
}

impl From<i32> for AttributeValue {
    fn from(n: i32) -> Self {
        Self::Integer(n)
    }
}

/// Collection of SVG attributes.
#[derive(Debug, Clone, Default)]
pub struct Attributes {
    attrs: Vec<Attribute>,
}

impl Attributes {
    /// Create a new empty attribute collection.
    #[must_use]
    pub fn new() -> Self {
        Self { attrs: Vec::new() }
    }

    /// Add an attribute.
    #[must_use]
    pub fn set<K: Into<String>, V: Into<AttributeValue>>(mut self, name: K, value: V) -> Self {
        self.attrs.push(Attribute {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Add a string attribute.
    #[must_use]
    pub fn str<K: Into<String>>(self, name: K, value: &str) -> Self {
        self.set(name, value)
    }

    /// Add a numeric attribute.
    #[must_use]
    pub fn num<K: Into<String>>(self, name: K, value: f32) -> Self {
        self.set(name, value)
    }

    /// Add an integer attribute.
    #[must_use]
    pub fn int<K: Into<String>>(self, name: K, value: i32) -> Self {
        self.set(name, value)
    }

    /// Add a data-* attribute.
    #[must_use]
    pub fn data(self, name: &str, value: &str) -> Self {
        self.set(format!("data-{name}"), value)
    }

    /// Add a class attribute (will be merged if multiple).
    #[must_use]
    pub fn class(mut self, class: &str) -> Self {
        // Look for existing class attribute and append
        for attr in &mut self.attrs {
            if attr.name == "class"
                && let AttributeValue::String(ref mut s) = attr.value
            {
                s.push(' ');
                s.push_str(class);
                return self;
            }
        }
        self.set("class", class)
    }

    /// Add an id attribute.
    #[must_use]
    pub fn id(self, id: &str) -> Self {
        self.set("id", id)
    }

    /// Check if a specific attribute is set.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.attrs.iter().any(|a| a.name == name)
    }

    /// Get the value of an attribute.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.attrs.iter().find(|a| a.name == name).map(|a| &a.value)
    }

    /// Render attributes to a string.
    #[must_use]
    pub fn render(&self) -> String {
        let mut result = String::new();
        for attr in &self.attrs {
            let _ = write!(result, " {}=\"{}\"", attr.name, attr.value);
        }
        result
    }

    /// Get the number of attributes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.attrs.len()
    }

    /// Check if the attribute collection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Merge another attribute collection into this one.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        for attr in other.attrs {
            if attr.name == "class" {
                // Merge classes
                if let AttributeValue::String(class) = &attr.value {
                    self = self.class(class);
                }
            } else {
                self.attrs.push(attr);
            }
        }
        self
    }
}

/// Escape special characters in XML attribute values.
fn escape_xml_attr(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

/// Escape special characters in XML text content.
pub fn escape_xml_text(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_attributes() {
        let attrs = Attributes::new()
            .set("id", "test")
            .set("width", 100.0_f32)
            .set("height", 50_i32);
        let rendered = attrs.render();
        assert!(rendered.contains("id=\"test\""));
        assert!(rendered.contains("width=\"100\""));
        assert!(rendered.contains("height=\"50\""));
    }

    #[test]
    fn escapes_special_characters() {
        let attrs = Attributes::new().set("title", "A & B < C > D \"E\" 'F'");
        let rendered = attrs.render();
        assert!(rendered.contains("&amp;"));
        assert!(rendered.contains("&lt;"));
        assert!(rendered.contains("&gt;"));
        assert!(rendered.contains("&quot;"));
        assert!(rendered.contains("&#39;"));
    }

    #[test]
    fn merges_classes() {
        let attrs = Attributes::new().class("foo").class("bar").class("baz");
        let rendered = attrs.render();
        assert!(rendered.contains("class=\"foo bar baz\""));
    }

    #[test]
    fn adds_data_attributes() {
        let attrs = Attributes::new().data("test", "value").data("count", "5");
        let rendered = attrs.render();
        assert!(rendered.contains("data-test=\"value\""));
        assert!(rendered.contains("data-count=\"5\""));
    }
}
