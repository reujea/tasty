use super::{
    CleanupReport, ProfileError,
    fs::{Dir, Namespace, Object},
    lease::{self, LeaseOps, Leases},
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
    fn regular_files(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| entry.child.as_ref().map_or(1, Self::regular_files))
            .sum()
    }
    fn lease_files(
        &self,
        leases: &mut Leases,
        seen: &mut HashSet<(u64, u64)>,
        ops: &LeaseOps<'_>,
    ) -> Result<(), ProfileError> {
        for entry in &self.entries {
            match &entry.child {
                Some(child) => child.lease_files(leases, seen, ops)?,
                None if seen.insert((entry.id.dev, entry.id.ino)) => {
                    leases.take(&self.dir, &entry.name, entry.id, ops)?;
                }
                None => {}
            }
        }
        Ok(())
    }
    /// Every regular file leased, then every lease confirmed: leasing and releasing them in turn
    /// would let a holder that alternates between two files pass.
    pub(super) fn lease_all(&self, ops: &LeaseOps<'_>) -> Result<Leases, ProfileError> {
        lease::survive_lease_breaks()?;
        let files = self.regular_files();
        if !lease::descriptors_available(files)? {
            return Err(std::io::Error::from_raw_os_error(libc::EMFILE).into());
        }
        let mut leases = Leases::default();
        self.lease_files(&mut leases, &mut HashSet::new(), ops)?;
        leases.confirm(ops)?;
        Ok(leases)
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
        let result = self.delete(namespace, name);
        if result.is_err() {
            self.recover_record(namespace, name, record);
        }
        result
    }
    fn delete(&self, namespace: &Namespace, name: &OsStr) -> Result<(), ProfileError> {
        let guard = || {
            namespace.validate()?;
            self.dir.check_link(&namespace.dir, name)
        };
        guard()?;
        self.remove(&guard)?;
        guard()?;
        namespace.dir.unlink(name, true)
    }
    /// A late writer can make rmdir fail after owner.json was removed. Restore only through the
    /// same validated root, never overwrite a replacement record.
    fn recover_record(&self, namespace: &Namespace, name: &OsStr, record: &Record) {
        let recovery = namespace
            .validate()
            .and_then(|()| self.dir.check_link(&namespace.dir, name))
            .and_then(|()| self.retain_record(record));
        if let Err(error) = recovery {
            tracing::warn!(%error, root_entry = ?name,
                device = self.dir.id.dev, inode = self.dir.id.ino,
                "private profile cleanup could not retain ownership record");
        }
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
    run_audited(parent, max_entries, &process::consumers, &lease::KERNEL)
}
/// Positive evidence of a consumer of a dead owner's root. Anything else leaves it to the leases.
type Audit<'a> = &'a dyn Fn(&Path, &HashSet<(u64, u64)>) -> std::io::Result<bool>;
pub(super) fn run_audited(
    parent: &Path,
    max_entries: usize,
    audit: Audit<'_>,
    ops: &LeaseOps<'_>,
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
        match candidate(&namespace, &name, audit, ops) {
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
    ops: &LeaseOps<'_>,
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
    reclaim_classified(namespace, name, dir, record, audit, ops)
}
fn reclaim_classified(
    namespace: &Namespace,
    name: &OsStr,
    dir: Dir,
    record: Record,
    audit: Audit<'_>,
    ops: &LeaseOps<'_>,
) -> Result<Outcome, ProfileError> {
    match record.owner.state(namespace.dir.file_system_type()) {
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
            tracing::debug!(%error, "private profile process audit incomplete; leases decide");
        }
    }
    dispose_rechecked(namespace, name, record, objects, ops)
}
fn dispose_rechecked(
    namespace: &Namespace,
    name: &OsStr,
    record: Record,
    objects: HashSet<(u64, u64)>,
    ops: &LeaseOps<'_>,
) -> Result<Outcome, ProfileError> {
    let state = record.owner.state(namespace.dir.file_system_type())?;
    if !matches!(state, OwnerState::Dead) {
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
    lease_and_delete(namespace, name, &record, tree, ops)
}
fn lease_and_delete(
    namespace: &Namespace,
    name: &OsStr,
    record: &Record,
    tree: Tree,
    ops: &LeaseOps<'_>,
) -> Result<Outcome, ProfileError> {
    // Any descriptor on these files other than the leases is closed by now. An open from here on
    // would break a lease of this process as well, so the record is not read again.
    let started = std::time::Instant::now();
    let leases = match tree.lease_all(ops) {
        Ok(leases) => leases,
        Err(error) => {
            log_lease_refusal(&error, tree.regular_files());
            return Ok(Outcome::Uncertain);
        }
    };
    log_leases_held(leases.len(), started);
    let result = tree.delete(namespace, name);
    drop(leases);
    if result.is_err() {
        tree.recover_record(namespace, name, record);
    }
    result?;
    Ok(Outcome::Removed)
}
fn log_lease_refusal(error: &ProfileError, files: usize) {
    if lease::held_elsewhere(error) {
        tracing::debug!(%error, files, "private profile file is held open; root retained");
    } else {
        tracing::warn!(%error, files, "private profile file lease unavailable; root retained");
    }
}
fn log_leases_held(files: usize, started: std::time::Instant) {
    tracing::debug!(
        files,
        elapsed_us = started.elapsed().as_micros() as u64,
        "private profile file leases held"
    );
}
