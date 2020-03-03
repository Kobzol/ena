/// A trait which allows actions (`T`) to be pushed which which allows the action to be undone at a
/// later time if needed
pub trait UndoLogs<T> {
    fn in_snapshot(&self) -> bool {
        self.num_open_snapshots() > 0
    }
    fn num_open_snapshots(&self) -> usize {
        0
    }
    fn push(&mut self, undo: T);
    fn clear(&mut self);
    fn extend<I>(&mut self, undos: I)
    where
        Self: Sized,
        I: IntoIterator<Item = T>,
    {
        undos.into_iter().for_each(|undo| self.push(undo));
    }
}

impl<'a, T, U> UndoLogs<T> for &'a mut U
where
    U: UndoLogs<T>,
{
    fn in_snapshot(&self) -> bool {
        (**self).in_snapshot()
    }
    fn num_open_snapshots(&self) -> usize {
        (**self).num_open_snapshots()
    }
    fn push(&mut self, undo: T) {
        (**self).push(undo)
    }
    fn clear(&mut self) {
        (**self).clear();
    }
    fn extend<I>(&mut self, undos: I)
    where
        Self: Sized,
        I: IntoIterator<Item = T>,
    {
        (**self).extend(undos)
    }
}

/// A trait which allows snapshots to be done at specific points. Each snapshot can then be used to
/// rollback any changes to an underlying data structures if they were not desirable.
///
/// Each snapshot must be consumed linearly with either `rollback_to` or `commit`.
pub trait Snapshots<T>: UndoLogs<T> {
    type Snapshot;
    fn has_changes(&self, snapshot: &Self::Snapshot) -> bool {
        !self.actions_since_snapshot(snapshot).is_empty()
    }
    fn actions_since_snapshot(&self, snapshot: &Self::Snapshot) -> &[T];

    fn start_snapshot(&mut self) -> Self::Snapshot;
    fn rollback_to<R>(&mut self, values: impl FnOnce() -> R, snapshot: Self::Snapshot)
    where
        R: Rollback<T>;

    fn commit(&mut self, snapshot: Self::Snapshot);
}

impl<T, U> Snapshots<T> for &'_ mut U
where
    U: Snapshots<T>,
{
    type Snapshot = U::Snapshot;
    fn has_changes(&self, snapshot: &Self::Snapshot) -> bool {
        (**self).has_changes(snapshot)
    }
    fn actions_since_snapshot(&self, snapshot: &Self::Snapshot) -> &[T] {
        (**self).actions_since_snapshot(snapshot)
    }

    fn start_snapshot(&mut self) -> Self::Snapshot {
        (**self).start_snapshot()
    }
    fn rollback_to<R>(&mut self, values: impl FnOnce() -> R, snapshot: Self::Snapshot)
    where
        R: Rollback<T>,
    {
        (**self).rollback_to(values, snapshot)
    }

    fn commit(&mut self, snapshot: Self::Snapshot) {
        (**self).commit(snapshot)
    }
}

pub struct NoUndo;
impl<T> UndoLogs<T> for NoUndo {
    fn num_open_snapshots(&self) -> usize {
        0
    }
    fn push(&mut self, _undo: T) {}
    fn clear(&mut self) {}
}

/// A basic undo log.
#[derive(Clone, Debug)]
pub struct VecLog<T> {
    log: Vec<T>,
    num_open_snapshots: usize,
}

impl<T> Default for VecLog<T> {
    fn default() -> Self {
        VecLog {
            log: Vec::new(),
            num_open_snapshots: 0,
        }
    }
}

impl<T> UndoLogs<T> for VecLog<T> {
    fn num_open_snapshots(&self) -> usize {
        self.num_open_snapshots
    }
    fn push(&mut self, undo: T) {
        self.log.push(undo);
    }
    fn clear(&mut self) {
        self.log.clear();
        self.num_open_snapshots = 0;
    }
}

impl<T> Snapshots<T> for VecLog<T> {
    type Snapshot = Snapshot;

    fn has_changes(&self, snapshot: &Self::Snapshot) -> bool {
        self.log.len() > snapshot.undo_len
    }
    fn actions_since_snapshot(&self, snapshot: &Snapshot) -> &[T] {
        &self.log[snapshot.undo_len..]
    }

    fn start_snapshot(&mut self) -> Snapshot {
        self.num_open_snapshots += 1;
        Snapshot {
            undo_len: self.log.len(),
        }
    }

    fn rollback_to<R>(&mut self, values: impl FnOnce() -> R, snapshot: Snapshot)
    where
        R: Rollback<T>,
    {
        debug!("rollback_to({})", snapshot.undo_len);

        self.assert_open_snapshot(&snapshot);

        if self.log.len() > snapshot.undo_len {
            let mut values = values();
            while self.log.len() > snapshot.undo_len {
                values.reverse(self.log.pop().unwrap());
            }
        }

        self.num_open_snapshots -= 1;
    }

    fn commit(&mut self, snapshot: Snapshot) {
        debug!("commit({})", snapshot.undo_len);

        self.assert_open_snapshot(&snapshot);

        if self.num_open_snapshots == 1 {
            // The root snapshot. It's safe to clear the undo log because
            // there's no snapshot further out that we might need to roll back
            // to.
            assert!(snapshot.undo_len == 0);
            self.log.clear();
        }

        self.num_open_snapshots -= 1;
    }
}

impl<T> VecLog<T> {
    fn assert_open_snapshot(&self, snapshot: &Snapshot) {
        // Failures here may indicate a failure to follow a stack discipline.
        assert!(self.log.len() >= snapshot.undo_len);
        assert!(self.num_open_snapshots > 0);
    }
}

impl<T> std::ops::Index<usize> for VecLog<T> {
    type Output = T;
    fn index(&self, key: usize) -> &T {
        &self.log[key]
    }
}

/// A trait implemented for types which can be rolled back using actions of type `U`.
pub trait Rollback<U> {
    fn reverse(&mut self, undo: U);
}

impl<T, U> Rollback<U> for &'_ mut T
where
    T: Rollback<U>,
{
    fn reverse(&mut self, undo: U) {
        (**self).reverse(undo)
    }
}

/// Snapshots are tokens that should be created/consumed linearly.
pub struct Snapshot {
    // Length of the undo log at the time the snapshot was taken.
    undo_len: usize,
}
