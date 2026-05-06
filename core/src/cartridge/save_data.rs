#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SaveDataError {
    NoExternalRam,
    NotBatteryBackedRam,
    SizeMismatch {
        expected_size: usize,
        actual_size: usize,
    },
}
