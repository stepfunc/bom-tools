pm::nothing!();

pub fn app() {
    shared::shared();
    common::common();
    #[cfg(feature = "use-alias")]
    alias::alias();
}
