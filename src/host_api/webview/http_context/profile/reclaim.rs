use super::{
    CleanupReport, ProfileError,
    fs::{Dir, Namespace, Object},
    process::{self, OwnerState},
    record::{self, Record},
};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    os::unix::fs::MetadataExt,
    path::Path,
};

const TREE_ENTRIES: usize = 65_536;
const TREE_DEPTH: usize = 64;
struct Entry {
    name: OsString,
    id: Object,
    child: Option<Tree>,
}
pub(super) struct Tree {
    dir: Dir,
    entries: Vec<Entry>,
}
impl Tree {
    pub(super) fn inspect(dir: Dir) -> Result<Self, ProfileError> {
        let mut budget = TREE_ENTRIES;
        Self::walk(dir, 0, &mut budget)
    }
    fn walk(dir: Dir, depth: usize, budget: &mut usize) -> Result<Self, ProfileError> {
        if depth > TREE_DEPTH {
            return Err(ProfileError::Capacity);
        }
        let (names, more) = dir.names(*budget, false)?;
        if more {
            return Err(ProfileError::Capacity);
        }
        let mut entries = Vec::new();
        for name in names {
            *budget = budget.checked_sub(1).ok_or(ProfileError::Capacity)?;
            let m = dir.metadata(&name)?;
            if m.uid() != super::fs::uid() || m.dev() != dir.id.dev || (!m.is_file() && !m.is_dir())
            {
                return Err(ProfileError::Ownership);
            }
            let id = Object::from(&m);
            let child = if m.is_dir() {
                let child = dir.open_dir(&name)?;
                if child.id != id {
                    return Err(ProfileError::Ownership);
                }
                Some(Self::walk(child, depth + 1, budget)?)
            } else {
                None
            };
            entries.push(Entry { name, id, child });
        }
        // Retain the root ownership record until other owned content has been removed.
        entries.sort_by_key(|e| e.name == record::NAME);
        Ok(Self { dir, entries })
    }
    pub(super) fn observations(&self, objects: &mut HashSet<(u64, u64)>) {
        objects.insert((self.dir.id.dev, self.dir.id.ino));
        for entry in &self.entries {
            objects.insert((entry.id.dev, entry.id.ino));
            if let Some(child) = &entry.child {
                child.observations(objects);
            }
        }
    }
    fn remove(&self, guard: &impl Fn() -> Result<(), ProfileError>) -> Result<(), ProfileError> {
        for entry in &self.entries {
            guard()?;
            if Object::from(&self.dir.metadata(&entry.name)?) != entry.id {
                return Err(ProfileError::Ownership);
            }
            if let Some(child) = &entry.child {
                child.remove(guard)?;
            }
            guard()?;
            if Object::from(&self.dir.metadata(&entry.name)?) != entry.id {
                return Err(ProfileError::Ownership);
            }
            self.dir.unlink(&entry.name, entry.child.is_some())?;
        }
        Ok(())
    }
    pub(super) fn dispose(
        &self,
        namespace: &Namespace,
        name: &OsStr,
        record: &Record,
    ) -> Result<(), ProfileError> {
        let guard = || {
            namespace.validate()?;
            self.dir.check_link(&namespace.dir, name)
        };
        guard()?;
        let result = (|| {
            self.remove(&guard)?;
            guard()?;
            namespace.dir.unlink(name, true)
        })();
        if result.is_err() {
            // A late writer can make rmdir fail after owner.json was removed. Restore only
            // through the same validated root, never overwrite a replacement record.
            let recovery = guard().and_then(|()| self.retain_record(record));
            if let Err(error) = recovery {
                tracing::warn!(%error, root_entry = ?name,
                    device = self.dir.id.dev, inode = self.dir.id.ino,
                    "private profile cleanup could not retain ownership record");
            }
        }
        result
    }
    fn retain_record(&self, record: &Record) -> Result<(), ProfileError> {
        match Record::read(&self.dir) {
            Ok(existing) if existing == *record => Ok(()),
            Err(ProfileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                record.write(&self.dir)
            }
            _ => Err(ProfileError::Ownership),
        }
    }
}
pub(super) fn remove_unpublished(
    namespace: &Namespace,
    name: &OsStr,
    dir: &Dir,
    record: &Record,
) -> Result<(), ProfileError> {
    namespace.validate()?;
    dir.check_link(&namespace.dir, name)?;
    let reopened = namespace.dir.open_dir(name)?;
    if reopened.id != dir.id {
        return Err(ProfileError::Ownership);
    }
    Tree::inspect(reopened)?.dispose(namespace, name, record)
}
pub(super) fn run(parent: &Path, max_entries: usize) -> Result<CleanupReport, ProfileError> {
    run_audited(parent, max_entries, &process::consumers)
}
/// Answers whether anything consumes a dead owner's root, or that the answer is unavailable.
type Audit<'a> = &'a dyn Fn(&Path, &HashSet<(u64, u64)>) -> std::io::Result<bool>;
fn run_audited(
    parent: &Path,
    max_entries: usize,
    audit: Audit<'_>,
) -> Result<CleanupReport, ProfileError> {
    if max_entries == 0 {
        return Err(ProfileError::Invalid);
    }
    let namespace = Namespace::open(parent)?;
    let _lock = namespace.lock()?;
    let (names, more) = namespace.entries(max_entries)?;
    let mut report = CleanupReport {
        limit_reached: more,
        ..Default::default()
    };
    for name in names {
        report.inspected += 1;
        match candidate(&namespace, &name, audit) {
            Ok(Outcome::Removed) => report.removed += 1,
            Ok(Outcome::Live) => report.kept_live += 1,
            Ok(Outcome::Uncertain) => report.kept_uncertain += 1,
            Err(error) => {
                tracing::warn!(%error, root_entry = ?name,
                    "private profile cleanup failed; remaining owned root retained");
                report.failed += 1;
            }
        }
    }
    Ok(report)
}
enum Outcome {
    Removed,
    Live,
    Uncertain,
}
fn candidate(
    namespace: &Namespace,
    name: &OsStr,
    audit: Audit<'_>,
) -> Result<Outcome, ProfileError> {
    let prepared = (|| {
        let dir = namespace.dir.open_dir(name)?;
        let record = Record::read(&dir)?;
        record.validate(namespace, &dir, &namespace.path.join(name))?;
        Ok::<_, ProfileError>((dir, record))
    })();
    let (dir, record) = match prepared {
        Ok(values) => values,
        Err(error) => {
            tracing::debug!(%error,"unclassified private profile entry retained");
            return Ok(Outcome::Uncertain);
        }
    };
    reclaim_classified(namespace, name, dir, record, audit)
}
fn reclaim_classified(
    namespace: &Namespace,
    name: &OsStr,
    dir: Dir,
    record: Record,
    audit: Audit<'_>,
) -> Result<Outcome, ProfileError> {
    match record.owner.state() {
        Ok(OwnerState::Live) => return Ok(Outcome::Live),
        Ok(OwnerState::Uncertain) => return Ok(Outcome::Uncertain),
        Err(error) => {
            tracing::warn!(%error,"private profile owner observation incomplete; root retained");
            return Ok(Outcome::Uncertain);
        }
        Ok(OwnerState::Dead) => {}
    }
    let tree = match Tree::inspect(dir) {
        Ok(tree) => tree,
        Err(error) => {
            tracing::debug!(%error,"private profile tree not safely inspectable; root retained");
            return Ok(Outcome::Uncertain);
        }
    };
    let mut objects = HashSet::new();
    tree.observations(&mut objects);
    // Close every internal candidate descriptor before auditing all threads. Otherwise shared
    // or unshared thread FD tables cannot safely distinguish internal handles from consumers.
    drop(tree);
    match audit(&namespace.path.join(name), &objects) {
        Ok(false) => {}
        Ok(true) => return Ok(Outcome::Uncertain),
        Err(error) => {
            tracing::warn!(%error,"private profile process audit incomplete; root retained");
            return Ok(Outcome::Uncertain);
        }
    }
    dispose_rechecked(namespace, name, record, objects)
}
fn dispose_rechecked(
    namespace: &Namespace,
    name: &OsStr,
    record: Record,
    objects: HashSet<(u64, u64)>,
) -> Result<Outcome, ProfileError> {
    if !matches!(record.owner.state()?, OwnerState::Dead) {
        return Ok(Outcome::Uncertain);
    }
    let dir = namespace.dir.open_dir(name)?;
    record.validate(namespace, &dir, &namespace.path.join(name))?;
    let tree = Tree::inspect(dir)?;
    let mut observed = HashSet::new();
    tree.observations(&mut observed);
    if observed != objects {
        return Ok(Outcome::Uncertain);
    }
    // Recheck the record as well as the anchored root immediately before deleting owned data.
    if Record::read(&tree.dir)? != record {
        return Ok(Outcome::Uncertain);
    }
    tree.dispose(namespace, name, &record)?;
    Ok(Outcome::Removed)
}
#[cfg(test)]
pub(super) fn run_with_audit(
    parent: &Path,
    audit: Audit<'_>,
) -> Result<CleanupReport, ProfileError> {
    run_audited(parent, 1, audit)
}
