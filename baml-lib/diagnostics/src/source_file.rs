use std::{fmt, sync::Arc};

/// A Prisma schema document.
#[derive(Clone)]
pub struct SourceFile {
    contents: Contents,
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SourceFile {}

impl SourceFile {
    pub fn new_static(content: &'static str) -> Self {
        Self {
            contents: Contents::Static(content),
        }
    }

    pub fn new_allocated(s: Arc<str>) -> Self {
        Self {
            contents: Contents::Allocated(s),
        }
    }

    pub fn as_str(&self) -> &str {
        match self.contents {
            Contents::Static(s) => s,
            Contents::Allocated(ref s) => s,
        }
    }
}

impl fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SourceFile {{ ... }}")?;

        Ok(())
    }
}

impl From<&str> for SourceFile {
    fn from(s: &str) -> Self {
        Self::new_allocated(Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<&String> for SourceFile {
    fn from(s: &String) -> Self {
        Self::new_allocated(Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<Box<str>> for SourceFile {
    fn from(s: Box<str>) -> Self {
        Self::new_allocated(Arc::from(s))
    }
}

impl From<Arc<str>> for SourceFile {
    fn from(s: Arc<str>) -> Self {
        Self::new_allocated(s)
    }
}

impl From<String> for SourceFile {
    fn from(s: String) -> Self {
        Self::new_allocated(Arc::from(s.into_boxed_str()))
    }
}

#[derive(Debug, Clone)]
enum Contents {
    Static(&'static str),
    Allocated(Arc<str>),
}
