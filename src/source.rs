//! Sumber data tunggal: Nimegami.
//!
//! Semua halaman diambil dari HTML server-rendered Nimegami lalu diurai oleh
//! [`crate::nimegami::NimegamiSource`]. Modul ini hanya menyediakan antarmuka
//! tipis agar handler web tidak bergantung pada detail implementasi.

use anyhow::Result;
use chrono::NaiveDate;

use crate::{
    models::{
        AnimeDetail, BrowsePage, BrowseParams, Card, EpisodeDetail, Genre, HomeData, ScheduleDay,
    },
    nimegami::NimegamiSource,
};

pub struct Source {
    inner: NimegamiSource,
}

impl Source {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            inner: NimegamiSource::new(base_url)?,
        })
    }

    pub fn host(&self) -> String {
        self.inner.host()
    }

    pub fn nimegami(&self) -> &NimegamiSource {
        &self.inner
    }

    pub async fn home(&self) -> Result<HomeData> {
        self.inner.home().await
    }

    /// Daftar kartu terbaru tanpa menunggu sumber eksternal seperti MAL.
    pub async fn latest(&self, limit: usize) -> Vec<Card> {
        self.inner.latest(limit).await
    }

    /// Rekomendasi dari snapshot cache saja (tanpa jaringan).
    pub fn latest_cached(&self, limit: usize) -> Vec<Card> {
        self.inner.latest_cached(limit)
    }

    pub async fn popular(&self, limit: usize) -> Result<Vec<Card>> {
        self.inner.popular(limit).await
    }

    pub async fn browse(&self, params: &BrowseParams) -> Result<BrowsePage> {
        self.inner.browse(params).await
    }

    pub async fn genres(&self) -> Result<Vec<Genre>> {
        self.inner.genres().await
    }

    pub async fn detail(&self, slug: &str) -> Result<AnimeDetail> {
        self.inner.detail(slug).await
    }

    pub async fn episode(&self, slug: &str) -> Result<EpisodeDetail> {
        self.inner.episode(slug).await
    }

    pub async fn schedule(&self, week_start: NaiveDate) -> Result<Vec<ScheduleDay>> {
        self.inner.schedule(week_start).await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        self.inner.search(query, limit).await
    }
}

/// Menormalkan URL gambar Nimegami.
///
/// Next.js mengirim poster lewat `/_next/image/?url=...&w=3840`, yang membuat
/// browser mengunduh berkas beresolusi penuh untuk kartu kecil. Fungsi ini
/// mengganti lebar optimizer ke ukuran yang masuk akal (384 untuk poster,
/// 1080 untuk backdrop) sambil tetap memakai optimizer resmi Nimegami.
pub fn resize_image(url: &str, width: u32, _height: u32, base_url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let origin = base_url.trim_end_matches('/');

    if trimmed.contains("/_next/image") {
        if let Some((_, after)) = trimmed.split_once("url=") {
            let encoded = after.split('&').next().unwrap_or("");
            if !encoded.is_empty() {
                return format!("{origin}/_next/image/?url={encoded}&w={width}&q=75");
            }
        }
    }

    if trimmed.starts_with('/') {
        return format!("{origin}{trimmed}");
    }

    trimmed.to_string()
}
