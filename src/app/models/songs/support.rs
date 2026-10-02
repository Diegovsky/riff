use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use crate::app::models::*;

// A range of numbers [a, b], empty range is allowed as well
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Range {
    Empty,
    NotEmpty(u32, u32),
}

impl Range {
    // Create a range [a, b] if b >= a, or an empty range otherwise
    fn of(a: impl TryInto<u32>, b: impl TryInto<u32>) -> Self {
        match (a.try_into(), b.try_into()) {
            (Ok(a), Ok(b)) if b >= a => Self::NotEmpty(a, b),
            _ => Self::Empty,
        }
    }

    fn len(self) -> u32 {
        match self {
            Self::Empty => 0,
            Self::NotEmpty(a, b) => b - a + 1,
        }
    }

    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::NotEmpty(a0, b0), Self::NotEmpty(a1, b1)) => {
                let start = u32::min(a0, a1);
                let end = u32::max(b0, b1);
                Self::NotEmpty(start, end)
            }
            (Self::Empty, r) | (r, Self::Empty) => r,
        }
    }

    fn offset_by(self, offset: i32) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::NotEmpty(a, b) => Self::of((a as i32) + offset, (b as i32) + offset),
        }
    }

    // Start index of the range, if not an empty range
    fn start<Target>(self) -> Option<Target>
    where
        Target: TryFrom<u32>,
    {
        match self {
            Self::Empty => None,
            Self::NotEmpty(a, _) => Some(a.try_into().ok()?),
        }
    }
}

// Represents the range affected by an operation on the list
// ListRangeUpdate(position, nb of elements added, nb of elements removed)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRangeUpdate(pub i32, pub i32, pub i32);

impl ListRangeUpdate {
    pub fn inserted(position: impl TryInto<i32>, added: impl TryInto<i32>) -> Self {
        Self(
            position.try_into().unwrap_or_default(),
            0,
            added.try_into().unwrap_or_default(),
        )
    }

    pub fn removed(position: impl TryInto<i32>, removed: impl TryInto<i32>) -> Self {
        Self(
            position.try_into().unwrap_or_default(),
            removed.try_into().unwrap_or_default(),
            0,
        )
    }

    pub fn updated(position: impl TryInto<i32>) -> Self {
        Self(position.try_into().unwrap_or_default(), 1, 1)
    }

    // Merge two range updates
    pub fn merge(self, other: Self) -> Self {
        // reorder for simplicity
        let (left, right) = if self.0 <= other.0 {
            (self, other)
        } else {
            (other, self)
        };

        let Self(p0, r0, a0) = left;
        let Self(p1, r1, a1) = right;

        // range [s, e] affected by first update
        let ra0 = Range::of(p0, p0 + r0 - 1);

        // ...second update, but only the range affecting existing elements
        let ra1 = {
            let s1 = i32::max(p0 + a0, p1);
            let e1 = i32::max(s1 - 1, p1 + r1 - 1);
            Range::of(s1, e1)
        };

        // remap to original
        let ra1 = ra1.offset_by(r0 - a0);

        // union
        let rau = ra0.union(ra1);

        let removed = rau.len() as i32;
        let position = rau.start().unwrap_or(p0);
        let added = removed - (r0 - a0) - (r1 - a1);
        Self(position, removed, added)
    }
}

// A list of songs that supports
// - batch loading (with non contiguous batches if songs are accessed in random order)
// - O(1) time access to a song by its id
// - manually adding content (not batched), when managing a queue for instance
// - tracking the affected range after a mutation
//
// Note: the mutated ranges are given in terms of LOADED tracks. The theoretical size of the list is not accounted for.
// This is to ease the work of updating the UI: we want to know what loaded/visible elements have moved around.
#[derive(Clone, Debug)]
pub struct SongList {
    total_loaded: usize,
    total: Option<usize>,
    batch_size: usize,
    last_batch_key: usize,
    complete: bool,
    // Why not a Vec? We could have batch 1, 2, NOT 3, then 4
    batches: HashMap<usize, Vec<SongModel>>,
    indexed_songs: HashMap<String, SongModel>,
}

impl SongList {
    pub fn new_sized(batch_size: usize) -> Self {
        Self {
            total_loaded: 0,
            total: None,
            batch_size,
            last_batch_key: 0,
            complete: false,
            batches: Default::default(),
            indexed_songs: Default::default(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.complete && !self.has_gap()
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn iter(&self) -> impl Iterator<Item = &SongModel> {
        self.iter_from(0).map(|(_, song)| song)
    }

    // How many songs we actually have at the moment
    pub fn partial_len(&self) -> usize {
        self.total_loaded
    }

    // How many songs are loaded, up to a given batch index
    fn estimated_len(&self, up_to_batch_index: usize) -> usize {
        let batches = &self.batches;
        let batch_size = self.batch_size;
        let batch_count = (0..up_to_batch_index)
            .filter(move |i| batches.contains_key(i))
            .count();
        batch_size * batch_count
    }

    // The theoretical len of the collection if we had all track
    pub fn len(&self) -> usize {
        self.total.unwrap_or(self.total_loaded)
    }

    // True if any batch request is still loading.
    pub fn has_gap(&self) -> bool {
        (0..self.last_batch_key).any(|i| !self.batches.contains_key(&i))
    }

    fn iter_from(&self, i: usize) -> impl Iterator<Item = (usize, &'_ SongModel)> {
        let batch_size = self.batch_size;
        let index = i / batch_size;
        self.iter_range(index, self.last_batch_key)
            .skip(i % batch_size)
    }

    // Find the position of a song in the list
    pub fn find_index(&self, song_id: &str) -> Option<usize> {
        self.iter_from(0)
            .find(|(_, song)| song.description().rri.id == song_id)
            .map(|(pos, _)| pos)
    }

    fn iter_range(&self, a: usize, b: usize) -> impl Iterator<Item = (usize, &'_ SongModel)> {
        let batch_size = self.batch_size;
        let batches = &self.batches;
        (a..=b)
            .filter_map(move |i| batches.get_key_value(&i))
            .flat_map(move |(k, b)| {
                b.iter()
                    .enumerate()
                    .map(move |(i, song)| (i + *k * batch_size, song))
            })
    }

    fn batches_add(
        batches: &mut HashMap<usize, Vec<SongModel>>,
        batch_size: usize,
        song: SongModel,
    ) {
        let index = batches.len().saturating_sub(1);
        let count = batches
            .get(&index)
            .map(|b| b.len() % batch_size)
            .unwrap_or(0);
        // If there's no space in the last batch, we insert a new one
        if count == 0 {
            batches.insert(batches.len(), vec![song]);
        } else {
            batches.get_mut(&index).unwrap().push(song);
        }
    }

    fn index_song(&mut self, song: &SongModel) {
        self.indexed_songs
            .entry(song.get_id())
            .or_insert_with(|| song.clone());
    }

    fn reindex(&mut self) {
        self.indexed_songs.clear();
        let songs: Vec<SongModel> = self.iter().cloned().collect();
        for song in &songs {
            self.index_song(song);
        }
    }

    fn repack(&mut self, songs: Vec<SongModel>) {
        let mut batches = HashMap::<usize, Vec<SongModel>>::default();
        self.total_loaded = songs.len();
        for song in songs {
            Self::batches_add(&mut batches, self.batch_size, song);
        }
        self.last_batch_key = batches.len().saturating_sub(1);
        self.batches = batches;
        self.reindex();
    }

    pub fn clear(&mut self) -> ListRangeUpdate {
        let len = self.partial_len();
        *self = Self::new_sized(self.batch_size);
        ListRangeUpdate::removed(0, len)
    }

    pub fn remove(&mut self, ids: &[String]) -> ListRangeUpdate {
        let len = self.total_loaded;
        let kept: Vec<SongModel> = self
            .iter()
            .filter(|song| !ids.contains(&song.description().rri.id))
            .cloned()
            .collect();
        self.repack(kept);
        // Lazy computation of the affected range, basically assume everything has changed
        ListRangeUpdate(0, len as i32, self.total_loaded as i32)
    }

    pub fn append(&mut self, songs: Vec<Track>) -> ListRangeUpdate {
        self.append_models(songs.into_iter().map(SongModel::new).collect())
    }

    pub fn append_models(&mut self, songs: Vec<SongModel>) -> ListRangeUpdate {
        let songs_len = songs.len();
        // How many loaded/visible songs so far
        let insertion_start = self.total_loaded;
        // Close any gap first
        if self.has_gap() || self.last_batch_key + 1 != self.batches.len() {
            let existing: Vec<SongModel> = self.iter().cloned().collect();
            self.repack(existing);
        }
        self.total_loaded = self.total_loaded.saturating_add(songs_len);
        // A directly-appended list is a fully-known, non-paginated collection.
        self.complete = true;
        for song in songs {
            self.index_song(&song);
            Self::batches_add(&mut self.batches, self.batch_size, song);
        }
        self.last_batch_key = self.batches.len().saturating_sub(1);
        ListRangeUpdate::inserted(insertion_start, songs_len)
    }

    pub fn replace_models(&mut self, songs: Vec<SongModel>) -> ListRangeUpdate {
        let len = self.total_loaded;
        *self = Self::new_sized(self.batch_size);
        self.complete = true;
        self.repack(songs);
        ListRangeUpdate(0, len as i32, self.total_loaded as i32)
    }

    pub fn prepend(&mut self, songs: Vec<Track>) -> ListRangeUpdate {
        let songs_len = songs.len();
        let insertion_start = 0;

        // Prepending also requires redoing all the batches
        let mut all: Vec<SongModel> = songs.into_iter().map(SongModel::new).collect();
        all.extend(self.iter().cloned());
        self.repack(all);

        // But it's a bit easier to computer the visibly affected range :)
        ListRangeUpdate::inserted(insertion_start, songs_len)
    }

    // Adding a batch is easy, might only require a resize
    // Add a loaded page. Pages are assumed aligned to this list's batch size
    // (every fetch uses the same limit), so the page drops in at
    // `offset / batch_size`.
    pub fn add(&mut self, batch: Page<Track>) -> Option<ListRangeUpdate> {
        let Page {
            items,
            offset,
            total,
            ..
        } = batch;
        let index = offset.unwrap_or(0) / self.batch_size;

        let insertion_start = self.estimated_len(index);
        let len = items.len();
        // A page shorter than the batch size (including an empty trailing page)
        // means we've reached the end of the collection.
        if len < self.batch_size {
            self.complete = true;
        }
        if let Some(total) = total {
            self.total = Some(self.total.map_or(total, |t| t.max(total)));
        }
        let songs: Vec<SongModel> = items.into_iter().map(SongModel::new).collect();
        for song in &songs {
            self.index_song(song);
        }

        if let Some(old) = self.batches.insert(index, songs) {
            // The same page loaded twice
            self.total_loaded -= old.len();
            self.reindex();
        }
        self.total_loaded += len;
        self.last_batch_key = usize::max(self.last_batch_key, index);

        Some(ListRangeUpdate::inserted(insertion_start, len))
    }

    fn index_mut(&mut self, i: usize) -> Option<&mut SongModel> {
        let batch_size = self.batch_size;
        let i_batch = i / batch_size;
        self.batches
            .get_mut(&i_batch)
            .and_then(|s| s.get_mut(i % batch_size))
    }

    pub fn swap(&mut self, a: usize, b: usize) -> Option<ListRangeUpdate> {
        if a == b {
            return None;
        }
        let a_value = self.index(a).cloned()?;
        let b_value = self.index(b).cloned()?;
        *self.index_mut(a)? = b_value;
        *self.index_mut(b)? = a_value;
        self.reindex();
        Some(ListRangeUpdate::updated(a).merge(ListRangeUpdate::updated(b)))
    }

    // Get the song at i (if the index is valid AND has been loaded)
    pub fn index(&self, i: usize) -> Option<&SongModel> {
        let batch_size = self.batch_size;
        let batch_id = i / batch_size;
        self.batches
            .get(&batch_id)
            .and_then(|batch| batch.get(i % batch_size))
    }

    // Get the i-th loaded song. VERY different!
    pub fn index_continuous(&self, i: usize) -> Option<&SongModel> {
        let batch_size = self.batch_size;
        let bi = i / batch_size;
        let batch = (0..=self.last_batch_key)
            // Skip missing/not loaded batches
            .filter_map(move |i| self.batches.get(&i))
            .nth(bi)?;
        batch.get(i % batch_size)
    }

    // Return the request needed to load the song at index i (if not loaded yet)
    pub fn needed_batch_for(&self, i: usize) -> Option<PageRequest> {
        let batch_size = self.batch_size;
        let batch_id = i / batch_size;
        if self.batches.contains_key(&batch_id) {
            None
        } else {
            Some(PageRequest {
                offset: batch_id * batch_size,
                batch_size,
            })
        }
    }

    // Get the full song batch that contains i
    pub fn song_batch_for(&self, i: usize) -> Option<Page<Track>> {
        let batch_size = self.batch_size;
        let batch_id = i / batch_size;
        self.batches.get(&batch_id).map(|songs| Page {
            items: songs.iter().map(|song| song.into_description()).collect(),
            offset: Some(batch_id * batch_size),
            total: None,
            next_cursor: None,
        })
    }

    pub fn get(&self, id: &str) -> Option<&SongModel> {
        self.indexed_songs.get(id)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const NO_CHANGE: ListRangeUpdate = ListRangeUpdate(0, 0, 0);

    impl SongList {
        fn new_from_initial_batch(initial: Page<Track>) -> Self {
            let mut s = Self::new_sized(2);
            s.add(initial);
            s
        }
    }

    fn song(id: &str) -> Track {
        make_track(id)
    }

    fn batch(id: usize) -> Page<Track> {
        let offset = id * 2;
        Page {
            items: vec![
                song(&format!("song{offset}")),
                song(&format!("song{}", offset + 1)),
            ],
            offset: Some(offset),
            total: None,
            next_cursor: None,
        }
    }

    #[test]
    fn test_merge_range() {
        // [0, 1, 2, 3, 4, 5]
        let change1 = ListRangeUpdate(0, 4, 2);
        // [x, x, 4, 5]
        let change2 = ListRangeUpdate(1, 1, 2);
        // [x, y, y, 4, 5]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 4, 3));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 4, 3));

        // [0, 1, 2, 3, 4, 5, 6]
        let change1 = ListRangeUpdate(0, 2, 3);
        // [x, x, x, 2, 3, 4, 5, 6]
        let change2 = ListRangeUpdate(4, 1, 1);
        // [x, x, x, 2, y, 4, 5, 6]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 4, 5));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 4, 5));

        // [0, 1, 2, 3, 4, 5, 6]
        let change1 = ListRangeUpdate(0, 3, 2);
        // [x, x, 3, 4, 5, 6]
        let change2 = ListRangeUpdate(4, 1, 1);
        // [x, x, 3, 4, y, 6]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 6, 5));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 6, 5));

        // [0, 1, 2, 3, 4, 5]
        let change1 = ListRangeUpdate(0, 4, 2);
        // [x, x, 4, 5]
        let change2 = ListRangeUpdate(1, 1, 1);
        // [x, y, 4, 5]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 4, 2));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 4, 2));

        // [0, 1, 2, 3, 4, 5]
        let change1 = ListRangeUpdate(0, 4, 2);
        // [x, x, 4, 5]
        let change2 = ListRangeUpdate(0, 4, 2);
        // [y, y]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 6, 2));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 6, 2));

        // []
        let change1 = ListRangeUpdate(0, 0, 2);
        // [x, x]
        let change2 = ListRangeUpdate(2, 0, 2);
        // [x, x, y, y]
        assert_eq!(change1.merge(change2), ListRangeUpdate(0, 0, 4));
        assert_eq!(change2.merge(change1), ListRangeUpdate(0, 0, 4));

        let change1 = ListRangeUpdate(0, 4, 2);
        assert_eq!(change1.merge(NO_CHANGE), ListRangeUpdate(0, 4, 2));
        assert_eq!(NO_CHANGE.merge(change1), ListRangeUpdate(0, 4, 2));
    }

    #[test]
    fn test_iter() {
        let list = SongList::new_from_initial_batch(batch(0));

        let mut list_iter = list.iter();
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song0");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song1");
        assert!(list_iter.next().is_none());
    }

    #[test]
    fn test_index() {
        let list = SongList::new_from_initial_batch(batch(0));

        let song1 = list.index(1);
        assert!(song1.is_some());

        let song3 = list.index(3);
        assert!(song3.is_none());
    }

    #[test]
    fn test_add() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(1));

        let song3 = list.index(3);
        assert!(song3.is_some());
        let list_iter = list.iter();
        assert_eq!(list_iter.count(), 4);
    }

    #[test]
    fn test_add_with_range() {
        let mut list = SongList::new_from_initial_batch(batch(0));

        let range = list.add(batch(1));
        assert_eq!(range, Some(ListRangeUpdate::inserted(2, 2)));
        assert_eq!(list.partial_len(), 4);

        let range = list.add(batch(3));
        assert_eq!(range, Some(ListRangeUpdate::inserted(4, 2)));
        assert_eq!(list.partial_len(), 6);

        let range = list.add(batch(2));
        assert_eq!(range, Some(ListRangeUpdate::inserted(4, 2)));
        assert_eq!(list.partial_len(), 8);
    }

    #[test]
    fn test_find_non_contiguous() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(3));

        let index = list.find_index("song6");

        assert_eq!(index, Some(6));
    }

    #[test]
    fn test_iter_non_contiguous() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(2));

        assert_eq!(list.partial_len(), 4);

        let mut list_iter = list.iter();
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song0");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song1");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song4");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song5");
        assert!(list_iter.next().is_none());
    }

    #[test]
    fn test_remove() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(1));

        list.remove(&["song0".to_string()]);

        assert_eq!(list.partial_len(), 3);

        let mut list_iter = list.iter();
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song1");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song2");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song3");
        assert!(list_iter.next().is_none());
    }

    #[test]
    fn test_remove_clears_lookup() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(1));

        assert!(list.get("song0").is_some());

        list.remove(&["song0".to_string()]);

        // `get` must not return songs that were removed, otherwise callers
        // relying on membership (e.g. liked-song checks) get stale results.
        assert!(list.get("song0").is_none());
        assert!(list.get("song1").is_some());
    }

    #[test]
    fn test_batch_for() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.add(batch(1));
        list.add(batch(2));
        list.add(batch(3));

        assert_eq!(list.partial_len(), 8);

        let batch = list.song_batch_for(3);
        assert_eq!(batch.unwrap().offset, Some(2));
    }

    #[test]
    fn test_append() {
        let mut list = SongList::new_from_initial_batch(batch(0));
        list.append(vec![song("song2")]);
        list.append(vec![song("song3")]);
        list.append(vec![song("song4")]);

        let mut list_iter = list.iter();
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song0");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song1");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song2");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song3");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song4");
        assert!(list_iter.next().is_none());
    }

    #[test]
    fn test_swap() {
        let mut list = SongList::new_sized(10);
        list.append(vec![song("song0"), song("song1"), song("song2")]);

        list.swap(0, 3); // should be a no-op
        list.swap(2, 3); // should be a no-op
        list.swap(0, 2);
        list.swap(0, 1);
        list.swap(2, 2); // should be no-op
        list.swap(2, 3); // should be no-op

        let mut list_iter = list.iter();
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song1");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song2");
        assert_eq!(list_iter.next().unwrap().description().rri.id, "song0");
        assert!(list_iter.next().is_none());
    }

    #[test]
    fn test_len_reports_api_total_when_known() {
        let mut list = SongList::new_sized(50);
        list.add(Page {
            items: (0..50).map(|i| song(&format!("song{i}"))).collect(),
            offset: Some(0),
            total: Some(1000),
            next_cursor: None,
        });

        assert_eq!(list.partial_len(), 50);
        assert_eq!(list.len(), 1000);
    }

    #[test]
    fn test_len_falls_back_to_loaded_count_when_total_unknown() {
        let mut list = SongList::new_sized(50);
        list.add(Page {
            items: (0..50).map(|i| song(&format!("song{i}"))).collect(),
            offset: Some(0),
            total: None,
            next_cursor: None,
        });

        assert_eq!(list.len(), 50);
    }

    #[test]
    fn test_len_keeps_max_total_seen() {
        let mut list = SongList::new_sized(50);
        list.add(Page {
            items: (0..50).map(|i| song(&format!("song{i}"))).collect(),
            offset: Some(0),
            total: Some(1000),
            next_cursor: None,
        });
        // A stale/racing response reporting a smaller total must not shrink len().
        list.add(Page {
            items: (50..100).map(|i| song(&format!("song{i}"))).collect(),
            offset: Some(50),
            total: Some(900),
            next_cursor: None,
        });

        assert_eq!(list.len(), 1000);
    }

    #[test]
    fn test_has_gap_detects_missing_batch() {
        let mut list = SongList::new_sized(2);
        list.add(batch(0));
        list.add(batch(2)); // batch 1 never arrived

        assert!(list.has_gap());
    }

    #[test]
    fn test_has_gap_false_when_contiguous() {
        let mut list = SongList::new_sized(2);
        list.add(batch(0));
        list.add(batch(1));

        assert!(!list.has_gap());
    }

    #[test]
    fn test_is_complete_false_while_gap_exists() {
        let mut list = SongList::new_sized(2);
        list.add(batch(0));
        // Short page at the tail marks `complete`, but a gap remains behind it.
        list.add(Page {
            items: vec![song("song4")],
            offset: Some(4),
            total: None,
            next_cursor: None,
        });

        assert!(!list.is_complete());
    }

    #[test]
    fn test_is_complete_true_once_gap_filled() {
        let mut list = SongList::new_sized(2);
        list.add(batch(0));
        list.add(Page {
            items: vec![song("song4")],
            offset: Some(4),
            total: None,
            next_cursor: None,
        });
        list.add(batch(1)); // fills the gap

        assert!(list.is_complete());
    }

    #[test]
    fn test_duplicates_get_their_own_models() {
        let mut list = SongList::new_sized(10);
        list.append(vec![song("a"), song("b"), song("a"), song("c")]);
        let first = list.index(0).unwrap().clone();
        let second = list.index(2).unwrap().clone();
        assert_ne!(first, second);
        assert_eq!(second.get_id(), "a");
        assert_eq!(list.get("a"), Some(&first));

        list.remove(&["a".to_string()]);
        let ids: Vec<String> = list.iter().map(|s| s.get_id()).collect();
        assert_eq!(ids, ["b", "c"]);
        assert_eq!(list.partial_len(), 2);
    }

    #[test]
    fn test_append_and_replace_models() {
        let mut list = SongList::new_sized(2);
        list.add(batch(3));
        list.append(vec![song("x")]);
        let ids: Vec<String> = list.iter().map(|s| s.get_id()).collect();
        assert_eq!(ids, ["song6", "song7", "x"]);
        assert_eq!(list.partial_len(), 3);

        let mut list = SongList::new_sized(2);
        let a = SongModel::new(song("a"));
        let b = SongModel::new(song("b"));
        list.replace_models(vec![a.clone(), b.clone()]);
        let range = list.replace_models(vec![b.clone(), a.clone(), b.clone()]);
        assert_eq!(range, ListRangeUpdate(0, 2, 3));
        assert_eq!(list.index_continuous(0), Some(&b));
        assert_eq!(list.index_continuous(1), Some(&a));
        assert_eq!(list.index_continuous(2), Some(&b));
    }
}
