mod cleanup;
pub use cleanup::{SourceDisposition, configure_cleanup, finish_cleanup, cleanup_in_progress};
mod parser;

mod analysis;
mod process;
pub use analysis::*;
pub use process::check as check_ffmpeg;
mod lookup;
pub use lookup::*;
mod planning;
pub use planning::*;
mod transfer;
pub use transfer::transfer;
#[cfg(test)]
mod tests;

pub fn clear_completed_sources(store: &crate::db::Store, plan: &crate::model::Plan) -> crate::model::Result<()> {
    if plan.kind != "cue_split" || plan.state != "complete" || !plan.metadata_errors.is_empty()
        || plan.rows.is_empty() || plan.rows.iter().any(|row| row.state != "complete" || row.action == "skip") {
        return Ok(());
    }
    let ctx = context(store, &plan.id)?;
    for file in std::iter::once(&ctx.analysis.cue).chain(ctx.analysis.sources.iter().map(|s| &s.file)) {
        let key = crate::paths::key(std::path::Path::new(&file.path));
        if store.get::<crate::model::SourceFile>("sources", &key)?.is_some_and(|s| s.signature == file.signature) {
            store.remove("sources", &key)?;
        }
    }
    Ok(())
}
