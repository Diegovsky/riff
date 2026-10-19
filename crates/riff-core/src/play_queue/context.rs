use std::collections::BTreeMap;

use riff_api::models::{Page, Track};

use super::{PageRequest, SongsSource};

/// Every page of a context is requested with this size.
pub const CONTEXT_PAGE_SIZE: usize = 50;

#[derive(Debug, Default)]
pub(super) struct Context {
    pub source: Option<SongsSource>,
    // By the position of their first track
    pages: BTreeMap<usize, Vec<Track>>,
    total: Option<usize>,
    end: Option<usize>,
}

impl Context {
    pub fn new(source: Option<SongsSource>) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    pub fn get(&self, i: usize) -> Option<&Track> {
        let (offset, page) = self.pages.range(..=i).next_back()?;
        page.get(i - offset)
    }

    fn len(&self) -> Option<usize> {
        self.total.or(self.end)
    }

    fn exists(&self, i: usize) -> bool {
        self.len().is_none_or(|len| i < len)
    }

    pub fn loaded(&self) -> impl Iterator<Item = usize> + '_ {
        self.pages
            .iter()
            .flat_map(|(offset, page)| *offset..offset + page.len())
    }

    pub fn add_page(&mut self, page: Page<Track>) -> Vec<usize> {
        let offset = page.offset.unwrap_or(0);
        let len = page.items.len();
        if let Some(total) = page.total {
            self.total = Some(total);
        }
        if len < CONTEXT_PAGE_SIZE {
            self.end = Some(offset + len);
        }
        if len == 0 || self.pages.contains_key(&offset) {
            return vec![];
        }
        self.pages.insert(offset, page.items);
        (offset..offset + len).collect()
    }

    pub fn set_all(&mut self, tracks: Vec<Track>) {
        let len = tracks.len();
        self.pages.clear();
        if len > 0 {
            self.pages.insert(0, tracks);
        }
        self.total = Some(len);
        self.end = Some(len);
    }

    pub fn missing_page(&self, from: usize, until: Option<usize>) -> Option<PageRequest> {
        let mut offset = from / CONTEXT_PAGE_SIZE * CONTEXT_PAGE_SIZE;
        while self.exists(offset) && until.is_none_or(|until| offset < until) {
            if self.get(offset).is_none() {
                return Some(PageRequest {
                    offset,
                    batch_size: CONTEXT_PAGE_SIZE,
                });
            }
            offset += CONTEXT_PAGE_SIZE;
        }
        None
    }
}
