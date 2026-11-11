//! Quick-toggle filter chips for the traffic table.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterChip {
    Hide304,
    HideImages,
    OnlyApi,
    ErrorsOnly,
}
