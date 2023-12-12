/// How mature/usable a member of an API is
///
/// Most things should be stable, however while spec is developed
/// we expect PROVISIONAL to be set.
#[derive(Debug, PartialEq, Copy, Clone, Hash, PartialOrd, Eq, Ord, Default)]
pub enum ApiMaturity {
    #[default]
    STABLE,
    PROVISIONAL,
    INTERNAL,
    DEPRECATED,
}
