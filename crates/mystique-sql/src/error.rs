/// A statement could not be built from the input (missing key, empty row,
/// unknown filter op, ...). The message is shown to the user as is.
///
/// The SPI maps this onto its query error, so the text is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError(String);

impl BuildError {
    pub fn new(message: String) -> Self {
        Self(message)
    }

    pub fn message(&self) -> &str {
        &self.0
    }

    pub fn into_message(self) -> String {
        self.0
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BuildError {}

pub type BuildResult<T> = Result<T, BuildError>;
