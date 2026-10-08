//! Music persistence (docs/features/music/data-model.md): tracks, the live
//! mix (channels, soundbites), soundscapes, playlists and soundboards.
//! Every write records a change (`music_track`, `music_live`, `music_soundscape`,
//! `music_playlist`, `music_soundboard`).

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, StoreError, changes, now};

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub source: String,
    pub url: Option<String>,
    pub blob_hash: Option<String>,
    pub title: String,
    pub notes: String,
    pub loop_by_default: bool,
    pub duration: Option<f64>,
    pub added_at: String,
    pub campaign_ids: Vec<String>,
    pub session_ids: Vec<String>,
    pub tags: Vec<String>,
}

/// Fully resolved track fields (the service stores uploads first).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackRow {
    pub source: String,
    pub url: Option<String>,
    pub blob_hash: Option<String>,
    pub title: String,
    pub notes: String,
    pub loop_by_default: bool,
    pub duration: Option<f64>,
    pub campaign_ids: Vec<String>,
    pub session_ids: Vec<String>,
    pub tags: Vec<String>,
    pub legacy_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackQuery {
    pub campaign_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub track_id: String,
    pub title: String,
    pub volume: i64,
    pub is_looping: bool,
    pub is_playing: bool,
    pub role: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Soundbite {
    pub id: String,
    pub track_id: String,
    pub title: String,
    pub volume: i64,
    pub min_interval: f64,
    pub max_interval: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundscapeChannel {
    pub track_id: String,
    pub title: String,
    pub volume: i64,
    pub is_looping: bool,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundscapeSoundbite {
    pub track_id: String,
    pub title: String,
    pub volume: i64,
    pub min_interval: f64,
    pub max_interval: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Soundscape {
    pub id: String,
    pub name: String,
    pub campaign_id: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub tags: Vec<String>,
    pub channels: Vec<SoundscapeChannel>,
    pub soundbites: Vec<SoundscapeSoundbite>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub track_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub campaign_id: Option<String>,
    pub created_at: String,
    pub items: Vec<PlaylistItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundboardButton {
    #[serde(default)]
    pub id: String,
    pub track_id: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub loop_by_default: bool,
    #[serde(default = "default_volume")]
    pub volume: i64,
    #[serde(default = "default_mode")]
    pub play_mode: String,
    #[serde(default)]
    pub hotkey: Option<String>,
}

fn default_volume() -> i64 {
    80
}
fn default_mode() -> String {
    "oneshot".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Soundboard {
    pub id: String,
    pub name: String,
    pub campaign_id: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub buttons: Vec<SoundboardButton>,
}

fn strings(conn: &Connection, sql: &str, id: &str) -> Result<Vec<String>> {
    let mut st = conn.prepare_cached(sql)?;
    Ok(st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

fn tag_id(conn: &Connection, name: &str) -> Result<i64> {
    let name = name.trim();
    if let Some(id) =
        conn.query_row("SELECT id FROM tags WHERE name = ?1", [name], |r| r.get(0)).optional()?
    {
        return Ok(id);
    }
    conn.execute("INSERT INTO tags (name) VALUES (?1)", [name])?;
    Ok(conn.last_insert_rowid())
}

fn track_from_row(conn: &Connection, r: &Row<'_>) -> rusqlite::Result<Track> {
    let id: String = r.get(0)?;
    let get = |sql: &str| strings(conn, sql, &id).unwrap_or_default();
    Ok(Track {
        campaign_ids: get("SELECT campaign_id FROM music_track_campaigns WHERE track_id = ?1"),
        session_ids: get("SELECT session_id FROM music_track_sessions WHERE track_id = ?1"),
        tags: get(
            "SELECT t.name FROM music_track_tags mt JOIN tags t ON t.id = mt.tag_id WHERE mt.track_id = ?1 ORDER BY t.name",
        ),
        id,
        source: r.get(1)?,
        url: r.get(2)?,
        blob_hash: r.get(3)?,
        title: r.get(4)?,
        notes: r.get(5)?,
        loop_by_default: r.get(6)?,
        duration: r.get(7)?,
        added_at: r.get(8)?,
    })
}

const TRACK_COLS: &str =
    "id, source, url, blob_hash, title, notes, loop_by_default, duration, added_at";

fn title_of(conn: &Connection, track_id: &str) -> String {
    conn.query_row("SELECT title FROM music_tracks WHERE id = ?1", [track_id], |r| r.get(0))
        .unwrap_or_default()
}

fn rec(conn: &Connection, entity: &str, id: &str, op: &str) -> Result<()> {
    changes::record(conn, entity, id, op)
}

impl Store {
    // ── tracks ──────────────────────────────────────────────────────────

    pub fn music_tracks_list(&self, q: &TrackQuery) -> Result<Vec<Track>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {TRACK_COLS} FROM music_tracks ORDER BY added_at DESC, rowid DESC"
        ))?;
        let all: Vec<Track> =
            st.query_map([], |r| track_from_row(&conn, r))?.collect::<rusqlite::Result<_>>()?;
        let text = q.text.as_deref().map(str::to_lowercase).filter(|t| !t.trim().is_empty());
        Ok(all
            .into_iter()
            .filter(|t| q.campaign_id.as_ref().is_none_or(|c| t.campaign_ids.contains(c)))
            .filter(|t| q.session_id.as_ref().is_none_or(|s| t.session_ids.contains(s)))
            .filter(|t| q.tags.iter().all(|g| t.tags.iter().any(|x| x.eq_ignore_ascii_case(g))))
            .filter(|t| text.as_ref().is_none_or(|x| t.title.to_lowercase().contains(x.trim())))
            .collect())
    }

    pub fn music_track_get(&self, id: &str) -> Result<Track> {
        let conn = self.conn();
        conn.query_row(&format!("SELECT {TRACK_COLS} FROM music_tracks WHERE id = ?1"), [id], |r| {
            track_from_row(&conn, r)
        })
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("track {id}")))
    }

    /// Finds a track by YouTube URL or upload blob (dedupe on import).
    pub fn music_track_find(
        &self,
        url: Option<&str>,
        blob: Option<&str>,
    ) -> Result<Option<String>> {
        let conn = self.conn();
        if let Some(u) = url.filter(|u| !u.is_empty())
            && let Some(id) = conn
                .query_row("SELECT id FROM music_tracks WHERE url = ?1", [u], |r| r.get(0))
                .optional()?
        {
            return Ok(Some(id));
        }
        if let Some(b) = blob.filter(|b| !b.is_empty()) {
            return Ok(conn
                .query_row("SELECT id FROM music_tracks WHERE blob_hash = ?1", [b], |r| r.get(0))
                .optional()?);
        }
        Ok(None)
    }

    pub fn music_track_by_legacy(&self, legacy: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT id FROM music_tracks WHERE legacy_id = ?1", [legacy], |r| r.get(0))
            .optional()?)
    }

    /// Inserts (`id = None`) or replaces a track's fields and links.
    pub fn music_track_save(&self, id: Option<&str>, row: &TrackRow) -> Result<Track> {
        let id = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let id = match id {
                Some(id) => {
                    let n = tx.execute(
                        "UPDATE music_tracks SET source=?2, url=?3, blob_hash=?4, title=?5, notes=?6, loop_by_default=?7, duration=?8 WHERE id=?1",
                        params![id, row.source, row.url, row.blob_hash, row.title, row.notes, row.loop_by_default, row.duration],
                    )?;
                    if n == 0 {
                        return Err(StoreError::NotFound(format!("track {id}")));
                    }
                    id.to_string()
                }
                None => {
                    let id = uuid();
                    tx.execute(
                        "INSERT INTO music_tracks (id, source, url, blob_hash, title, notes, loop_by_default, duration, added_at, legacy_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                        params![id, row.source, row.url, row.blob_hash, row.title, row.notes, row.loop_by_default, row.duration, now(), row.legacy_id],
                    )?;
                    id
                }
            };
            for t in ["music_track_campaigns", "music_track_sessions", "music_track_tags"] {
                tx.execute(&format!("DELETE FROM {t} WHERE track_id = ?1"), [&id])?;
            }
            for c in &row.campaign_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO music_track_campaigns VALUES (?1, ?2)",
                    params![id, c],
                )?;
            }
            for s in &row.session_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO music_track_sessions VALUES (?1, ?2)",
                    params![id, s],
                )?;
            }
            for t in row.tags.iter().filter(|t| !t.trim().is_empty()) {
                let tid = tag_id(&tx, t)?;
                tx.execute(
                    "INSERT OR IGNORE INTO music_track_tags VALUES (?1, ?2)",
                    params![id, tid],
                )?;
            }
            rec(&tx, "music_track", &id, "update")?;
            tx.commit()?;
            id
        };
        self.music_track_get(&id)
    }

    pub fn music_track_set_duration(&self, id: &str, seconds: f64) -> Result<()> {
        let conn = self.conn();
        if conn.execute(
            "UPDATE music_tracks SET duration = ?2 WHERE id = ?1 AND duration IS NULL",
            params![id, seconds],
        )? > 0
        {
            rec(&conn, "music_track", id, "update")?;
        }
        Ok(())
    }

    pub fn music_track_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM music_tracks WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("track {id}")));
        }
        rec(&conn, "music_track", id, "delete")?;
        rec(&conn, "music_live", "live", "update")
    }

    // ── live mix ────────────────────────────────────────────────────────

    pub fn music_channels(&self) -> Result<Vec<Channel>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT c.id, c.track_id, t.title, c.volume, c.is_looping, c.is_playing, c.role, c.position
             FROM music_channels c JOIN music_tracks t ON t.id = c.track_id ORDER BY c.position, c.rowid",
        )?;
        Ok(st
            .query_map([], |r| {
                Ok(Channel {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    title: r.get(2)?,
                    volume: r.get(3)?,
                    is_looping: r.get(4)?,
                    is_playing: r.get(5)?,
                    role: r.get(6)?,
                    position: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn music_soundbites(&self) -> Result<Vec<Soundbite>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT s.id, s.track_id, t.title, s.volume, s.min_interval, s.max_interval
             FROM music_soundbites s JOIN music_tracks t ON t.id = s.track_id ORDER BY s.position, s.rowid",
        )?;
        Ok(st
            .query_map([], |r| {
                Ok(Soundbite {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    title: r.get(2)?,
                    volume: r.get(3)?,
                    min_interval: r.get(4)?,
                    max_interval: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn music_channel_insert(
        &self,
        track_id: &str,
        volume: i64,
        looping: bool,
        playing: bool,
        role: &str,
    ) -> Result<String> {
        let conn = self.conn();
        let id = uuid();
        let pos: i64 =
            conn.query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM music_channels", [], |r| {
                r.get(0)
            })?;
        conn.execute(
            "INSERT INTO music_channels (id, track_id, volume, is_looping, is_playing, role, position) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, track_id, volume, looping, playing, role, pos],
        )?;
        rec(&conn, "music_live", "live", "update")?;
        Ok(id)
    }

    pub fn music_channel_update(&self, c: &Channel) -> Result<()> {
        let conn = self.conn();
        if conn.execute(
            "UPDATE music_channels SET volume=?2, is_looping=?3, is_playing=?4, role=?5 WHERE id=?1",
            params![c.id, c.volume, c.is_looping, c.is_playing, c.role],
        )? == 0
        {
            return Err(StoreError::NotFound(format!("channel {}", c.id)));
        }
        rec(&conn, "music_live", "live", "update")
    }

    pub fn music_channel_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM music_channels WHERE id = ?1", [id])?;
        rec(&conn, "music_live", "live", "update")
    }

    pub fn music_channels_set_playing(&self, playing: bool) -> Result<()> {
        let conn = self.conn();
        conn.execute("UPDATE music_channels SET is_playing = ?1", [playing])?;
        rec(&conn, "music_live", "live", "update")
    }

    pub fn music_soundbite_insert(
        &self,
        track_id: &str,
        volume: i64,
        min: f64,
        max: f64,
    ) -> Result<String> {
        let conn = self.conn();
        let id = uuid();
        let pos: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM music_soundbites",
            [],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO music_soundbites (id, track_id, volume, min_interval, max_interval, position) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, track_id, volume, min, max, pos],
        )?;
        rec(&conn, "music_live", "live", "update")?;
        Ok(id)
    }

    pub fn music_soundbite_update(&self, s: &Soundbite) -> Result<()> {
        let conn = self.conn();
        if conn.execute(
            "UPDATE music_soundbites SET volume=?2, min_interval=?3, max_interval=?4 WHERE id=?1",
            params![s.id, s.volume, s.min_interval, s.max_interval],
        )? == 0
        {
            return Err(StoreError::NotFound(format!("soundbite {}", s.id)));
        }
        rec(&conn, "music_live", "live", "update")
    }

    pub fn music_soundbite_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM music_soundbites WHERE id = ?1", [id])?;
        rec(&conn, "music_live", "live", "update")
    }

    /// Wipes the live channels and soundbites (global stop, startup).
    pub fn music_clear_live(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch("DELETE FROM music_channels; DELETE FROM music_soundbites;")?;
        rec(&conn, "music_live", "live", "update")
    }

    // ── soundscapes ─────────────────────────────────────────────────────

    fn soundscape_full(conn: &Connection, id: &str) -> Result<Soundscape> {
        let (name, campaign_id, notes, created_at) = conn
            .query_row(
                "SELECT name, campaign_id, notes, created_at FROM music_soundscapes WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("soundscape {id}")))?;
        let mut st = conn.prepare_cached(
            "SELECT c.track_id, t.title, c.volume, c.is_looping, c.role FROM music_soundscape_channels c
             JOIN music_tracks t ON t.id = c.track_id WHERE c.soundscape_id = ?1 ORDER BY c.position",
        )?;
        let channels = st
            .query_map([id], |r| {
                Ok(SoundscapeChannel {
                    track_id: r.get(0)?,
                    title: r.get(1)?,
                    volume: r.get(2)?,
                    is_looping: r.get(3)?,
                    role: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut st = conn.prepare_cached(
            "SELECT s.track_id, t.title, s.volume, s.min_interval, s.max_interval FROM music_soundscape_soundbites s
             JOIN music_tracks t ON t.id = s.track_id WHERE s.soundscape_id = ?1 ORDER BY s.position",
        )?;
        let soundbites = st
            .query_map([id], |r| {
                Ok(SoundscapeSoundbite {
                    track_id: r.get(0)?,
                    title: r.get(1)?,
                    volume: r.get(2)?,
                    min_interval: r.get(3)?,
                    max_interval: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Soundscape {
            id: id.to_string(),
            name,
            campaign_id,
            notes,
            created_at,
            tags: strings(
                conn,
                "SELECT t.name FROM music_soundscape_tags s JOIN tags t ON t.id = s.tag_id WHERE s.soundscape_id = ?1 ORDER BY t.name",
                id,
            )?,
            channels,
            soundbites,
        })
    }

    pub fn music_soundscapes(&self, campaign_id: Option<&str>) -> Result<Vec<Soundscape>> {
        let conn = self.conn();
        let ids = {
            let mut st = conn.prepare(
                "SELECT id, campaign_id FROM music_soundscapes ORDER BY name COLLATE NOCASE",
            )?;
            st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        ids.into_iter()
            .filter(|(_, c)| campaign_id.is_none_or(|w| c.as_deref() == Some(w)))
            .map(|(id, _)| Self::soundscape_full(&conn, &id))
            .collect()
    }

    pub fn music_soundscape_get(&self, id: &str) -> Result<Soundscape> {
        Self::soundscape_full(&self.conn(), id)
    }

    /// Creates a soundscape (or overwrites `id`'s mix) from explicit content.
    #[allow(clippy::too_many_arguments)]
    pub fn music_soundscape_write(
        &self,
        id: Option<&str>,
        name: &str,
        campaign_id: Option<&str>,
        notes: &str,
        tags: &[String],
        channels: &[SoundscapeChannel],
        soundbites: &[SoundscapeSoundbite],
    ) -> Result<String> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let id = match id {
            Some(id) => {
                tx.execute(
                    "UPDATE music_soundscapes SET name=?2, campaign_id=?3, notes=?4 WHERE id=?1",
                    params![id, name, campaign_id, notes],
                )?;
                id.to_string()
            }
            None => {
                let id = uuid();
                tx.execute(
                    "INSERT INTO music_soundscapes (id, name, campaign_id, notes, created_at) VALUES (?1,?2,?3,?4,?5)",
                    params![id, name, campaign_id, notes, now()],
                )?;
                id
            }
        };
        for t in
            ["music_soundscape_channels", "music_soundscape_soundbites", "music_soundscape_tags"]
        {
            tx.execute(&format!("DELETE FROM {t} WHERE soundscape_id = ?1"), [&id])?;
        }
        for (i, c) in channels.iter().enumerate() {
            tx.execute(
                "INSERT INTO music_soundscape_channels (id, soundscape_id, track_id, volume, is_looping, role, position) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![uuid(), id, c.track_id, c.volume, c.is_looping, c.role, i as i64],
            )?;
        }
        for (i, s) in soundbites.iter().enumerate() {
            tx.execute(
                "INSERT INTO music_soundscape_soundbites (id, soundscape_id, track_id, volume, min_interval, max_interval, position) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![uuid(), id, s.track_id, s.volume, s.min_interval, s.max_interval, i as i64],
            )?;
        }
        for t in tags.iter().filter(|t| !t.trim().is_empty()) {
            let tid = tag_id(&tx, t)?;
            tx.execute(
                "INSERT OR IGNORE INTO music_soundscape_tags VALUES (?1, ?2)",
                params![id, tid],
            )?;
        }
        rec(&tx, "music_soundscape", &id, "update")?;
        tx.commit()?;
        Ok(id)
    }

    pub fn music_soundscape_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM music_soundscapes WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("soundscape {id}")));
        }
        rec(&conn, "music_soundscape", id, "delete")
    }

    /// Replaces the live mix with the soundscape's channels (`is_playing`) and soundbites.
    pub fn music_soundscape_to_live(&self, id: &str, playing: bool) -> Result<()> {
        let s = self.music_soundscape_get(id)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute_batch("DELETE FROM music_channels; DELETE FROM music_soundbites;")?;
        for (i, c) in s.channels.iter().enumerate() {
            tx.execute(
                "INSERT INTO music_channels (id, track_id, volume, is_looping, is_playing, role, position) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![uuid(), c.track_id, c.volume, c.is_looping, playing, c.role, i as i64],
            )?;
        }
        for (i, b) in s.soundbites.iter().enumerate() {
            tx.execute(
                "INSERT INTO music_soundbites (id, track_id, volume, min_interval, max_interval, position) VALUES (?1,?2,?3,?4,?5,?6)",
                params![uuid(), b.track_id, b.volume, b.min_interval, b.max_interval, i as i64],
            )?;
        }
        rec(&tx, "music_live", "live", "update")?;
        tx.commit()?;
        Ok(())
    }

    // ── playlists ───────────────────────────────────────────────────────

    fn playlist_full(conn: &Connection, id: &str) -> Result<Playlist> {
        let (name, campaign_id, created_at) = conn
            .query_row(
                "SELECT name, campaign_id, created_at FROM music_playlists WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("playlist {id}")))?;
        let mut st = conn.prepare_cached(
            "SELECT i.id, i.track_id, t.title FROM music_playlist_items i JOIN music_tracks t ON t.id = i.track_id
             WHERE i.playlist_id = ?1 ORDER BY i.position",
        )?;
        let items = st
            .query_map([id], |r| {
                Ok(PlaylistItem { id: r.get(0)?, track_id: r.get(1)?, title: r.get(2)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Playlist { id: id.to_string(), name, campaign_id, created_at, items })
    }

    pub fn music_playlists(&self, campaign_id: Option<&str>) -> Result<Vec<Playlist>> {
        let conn = self.conn();
        let ids = {
            let mut st = conn.prepare(
                "SELECT id, campaign_id FROM music_playlists ORDER BY name COLLATE NOCASE",
            )?;
            st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        ids.into_iter()
            .filter(|(_, c)| campaign_id.is_none_or(|w| c.as_deref() == Some(w)))
            .map(|(id, _)| Self::playlist_full(&conn, &id))
            .collect()
    }

    pub fn music_playlist_get(&self, id: &str) -> Result<Playlist> {
        Self::playlist_full(&self.conn(), id)
    }

    pub fn music_playlist_save(
        &self,
        id: Option<&str>,
        name: &str,
        campaign_id: Option<&str>,
        track_ids: &[String],
    ) -> Result<Playlist> {
        let id = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let id = match id {
                Some(id) => {
                    if tx.execute(
                        "UPDATE music_playlists SET name=?2, campaign_id=?3 WHERE id=?1",
                        params![id, name, campaign_id],
                    )? == 0
                    {
                        return Err(StoreError::NotFound(format!("playlist {id}")));
                    }
                    id.to_string()
                }
                None => {
                    let id = uuid();
                    tx.execute(
                        "INSERT INTO music_playlists (id, name, campaign_id, created_at) VALUES (?1,?2,?3,?4)",
                        params![id, name, campaign_id, now()],
                    )?;
                    id
                }
            };
            tx.execute("DELETE FROM music_playlist_items WHERE playlist_id = ?1", [&id])?;
            for (i, t) in track_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO music_playlist_items (id, playlist_id, track_id, position) VALUES (?1,?2,?3,?4)",
                    params![uuid(), id, t, i as i64],
                )?;
            }
            rec(&tx, "music_playlist", &id, "update")?;
            tx.commit()?;
            id
        };
        self.music_playlist_get(&id)
    }

    pub fn music_playlist_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM music_playlists WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("playlist {id}")));
        }
        rec(&conn, "music_playlist", id, "delete")
    }

    // ── soundboards ─────────────────────────────────────────────────────

    fn soundboard_full(conn: &Connection, id: &str) -> Result<Soundboard> {
        let (name, campaign_id, notes, created_at) = conn
            .query_row(
                "SELECT name, campaign_id, notes, created_at FROM music_soundboards WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("soundboard {id}")))?;
        let mut st = conn.prepare_cached(
            "SELECT id, track_id, label, icon, color, position, loop_by_default, volume, play_mode, hotkey
             FROM music_soundboard_buttons WHERE soundboard_id = ?1 ORDER BY position",
        )?;
        let buttons = st
            .query_map([id], |r| {
                Ok(SoundboardButton {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    label: r.get(2)?,
                    icon: r.get(3)?,
                    color: r.get(4)?,
                    position: r.get(5)?,
                    loop_by_default: r.get(6)?,
                    volume: r.get(7)?,
                    play_mode: r.get(8)?,
                    hotkey: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Soundboard { id: id.to_string(), name, campaign_id, notes, created_at, buttons })
    }

    pub fn music_soundboards(&self, campaign_id: Option<&str>) -> Result<Vec<Soundboard>> {
        let conn = self.conn();
        let ids = {
            let mut st = conn.prepare(
                "SELECT id, campaign_id FROM music_soundboards ORDER BY name COLLATE NOCASE",
            )?;
            st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        ids.into_iter()
            .filter(|(_, c)| campaign_id.is_none_or(|w| c.as_deref() == Some(w)))
            .map(|(id, _)| Self::soundboard_full(&conn, &id))
            .collect()
    }

    /// Creates or updates a board; buttons are upserted (stable ids), removed
    /// buttons deleted, positions follow the given order.
    pub fn music_soundboard_save(
        &self,
        id: Option<&str>,
        name: &str,
        campaign_id: Option<&str>,
        notes: &str,
        buttons: &[SoundboardButton],
    ) -> Result<Soundboard> {
        let id = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let id = match id {
                Some(id) => {
                    if tx.execute(
                        "UPDATE music_soundboards SET name=?2, campaign_id=?3, notes=?4 WHERE id=?1",
                        params![id, name, campaign_id, notes],
                    )? == 0
                    {
                        return Err(StoreError::NotFound(format!("soundboard {id}")));
                    }
                    id.to_string()
                }
                None => {
                    let id = uuid();
                    tx.execute(
                        "INSERT INTO music_soundboards (id, name, campaign_id, notes, created_at) VALUES (?1,?2,?3,?4,?5)",
                        params![id, name, campaign_id, notes, now()],
                    )?;
                    id
                }
            };
            let mut keep = Vec::new();
            for (i, b) in buttons.iter().enumerate() {
                let bid = if b.id.is_empty() { uuid() } else { b.id.clone() };
                tx.execute(
                    "INSERT INTO music_soundboard_buttons (id, soundboard_id, track_id, label, icon, color, position, loop_by_default, volume, play_mode, hotkey)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                     ON CONFLICT(id) DO UPDATE SET track_id=excluded.track_id, label=excluded.label, icon=excluded.icon, color=excluded.color,
                       position=excluded.position, loop_by_default=excluded.loop_by_default, volume=excluded.volume,
                       play_mode=excluded.play_mode, hotkey=excluded.hotkey",
                    params![bid, id, b.track_id, b.label, b.icon, b.color, i as i64, b.loop_by_default, b.volume.clamp(0, 100), b.play_mode, b.hotkey],
                )?;
                keep.push(bid);
            }
            let existing = strings(
                &tx,
                "SELECT id FROM music_soundboard_buttons WHERE soundboard_id = ?1",
                &id,
            )?;
            for e in existing.iter().filter(|e| !keep.contains(e)) {
                tx.execute("DELETE FROM music_soundboard_buttons WHERE id = ?1", [e])?;
            }
            rec(&tx, "music_soundboard", &id, "update")?;
            tx.commit()?;
            id
        };
        Self::soundboard_full(&self.conn(), &id)
    }

    pub fn music_soundboard_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM music_soundboards WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("soundboard {id}")));
        }
        rec(&conn, "music_soundboard", id, "delete")
    }

    pub fn music_button_get(&self, id: &str) -> Result<SoundboardButton> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id, track_id, label, icon, color, position, loop_by_default, volume, play_mode, hotkey FROM music_soundboard_buttons WHERE id = ?1",
            [id],
            |r| {
                Ok(SoundboardButton {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    label: r.get(2)?,
                    icon: r.get(3)?,
                    color: r.get(4)?,
                    position: r.get(5)?,
                    loop_by_default: r.get(6)?,
                    volume: r.get(7)?,
                    play_mode: r.get(8)?,
                    hotkey: r.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("button {id}")))
    }

    /// Title of a track (empty if missing).
    pub fn music_track_title(&self, id: &str) -> String {
        title_of(&self.conn(), id)
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_store;

    use super::*;

    #[test]
    fn tracks_live_mix_and_soundscapes() {
        let (_d, s) = temp_store();
        let c = s.create_campaign("Strahd").unwrap();
        let row = TrackRow {
            source: "youtube".into(),
            url: Some("https://youtu.be/x".into()),
            title: "Rain".into(),
            loop_by_default: true,
            campaign_ids: vec![c.id.clone()],
            tags: vec!["weather".into()],
            ..Default::default()
        };
        let t = s.music_track_save(None, &row).unwrap();
        assert_eq!(t.tags, vec!["weather"]);
        assert_eq!(
            s.music_tracks_list(&TrackQuery { tags: vec!["weather".into()], ..Default::default() })
                .unwrap()
                .len(),
            1
        );
        s.music_channel_insert(&t.id, 80, true, true, "ambient").unwrap();
        s.music_soundbite_insert(&t.id, 70, 5.0, 9.0).unwrap();
        let ch: Vec<SoundscapeChannel> = s
            .music_channels()
            .unwrap()
            .iter()
            .map(|c| SoundscapeChannel {
                track_id: c.track_id.clone(),
                title: c.title.clone(),
                volume: c.volume,
                is_looping: c.is_looping,
                role: c.role.clone(),
            })
            .collect();
        let id = s.music_soundscape_write(None, "Storm", None, "", &[], &ch, &[]).unwrap();
        s.music_clear_live().unwrap();
        s.music_soundscape_to_live(&id, true).unwrap();
        assert_eq!(s.music_channels().unwrap().len(), 1);
        // deleting the campaign keeps the track
        s.delete_campaign(&c.id).unwrap();
        assert!(s.music_track_get(&t.id).is_ok());
    }
}
