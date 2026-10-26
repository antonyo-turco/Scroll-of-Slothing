//! Music service tests (spec: docs/features/music/*.md, checklist in overview.md).
use super::*;
use crate::temp_service;

fn engine_cmds(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<Value> {
    let mut out = vec![];
    while let Ok(e) = rx.try_recv() {
        if let Event::MusicEngine { command } = e {
            out.push(command);
        }
    }
    out
}

async fn yt_track(svc: &Service, title: &str) -> Track {
    svc.music_track_create(TrackInput {
        source: "youtube".into(),
        url: Some(format!("https://youtu.be/{title}")),
        file_path: None,
        title: title.into(),
        notes: None,
        loop_by_default: Some(true),
        duration: Some(3.0),
        tags: vec!["mood".into()],
        category: None,
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn mixer_drives_the_engine_with_ducking_and_soundscapes() {
    let (d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let mut rx = svc.subscribe();
    let t = yt_track(&svc, "rain").await;
    let st = svc
        .music_channel_add(t.id.clone(), Some("ambient".into()), Some(60), None, true)
        .await
        .unwrap();
    let cmds = engine_cmds(&mut rx);
    assert!(cmds.iter().any(|c| c["action"] == "play_track" && c["volume"] == 60), "{cmds:?}");
    let ch = st.channels[0].id.clone();
    svc.music_set_master(50).await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "set_volume" && c["volume"] == 30));
    svc.music_channel_update(
        ch.clone(),
        ChannelPatch { is_playing: Some(false), ..Default::default() },
    )
    .await
    .unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "pause_channel"));

    let sc = svc
        .music_soundscape_save(SoundscapeInput { name: "Storm".into(), notes: None, tags: vec![] })
        .await
        .unwrap();
    svc.music_stop_all().await.unwrap();
    let st = svc.music_soundscape_load(sc.id.clone()).await.unwrap();
    assert_eq!((st.channels.len(), st.soundscape_name.as_deref()), (1, Some("Storm")));
    let cmds = engine_cmds(&mut rx);
    assert!(cmds.iter().any(|c| c["action"] == "stop_all"));
    assert!(cmds.iter().any(|c| c["action"] == "play_track"));

    // .dmix round trip
    let p = d.path().join("Storm night.dmix");
    svc.music_mix_export(p.clone(), None).await.unwrap();
    let r = svc.music_mix_import(p, true).await.unwrap();
    assert_eq!((r.name.as_str(), r.tracks_reused, r.tracks_created), ("Storm night", 1, 0));
    assert_eq!(r.state.channels.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn playlist_advances_and_soundboard_toggles() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let a = yt_track(&svc, "a").await;
    let b = yt_track(&svc, "b").await;
    let pl = svc
        .music_playlist_save(PlaylistInput {
            id: None,
            name: "Set".into(),
            track_ids: vec![a.id.clone(), b.id.clone(), a.id.clone()],
        })
        .await
        .unwrap();
    assert_eq!(pl.items.len(), 3, "duplicates allowed");
    svc.music_playlist_play(pl.id.clone(), None).await.unwrap();
    svc.music_engine_event(EngineEvent::ChannelEnded { channel_id: "playlist".into() })
        .await
        .unwrap();
    assert_eq!(svc.music_state().await.unwrap().playlist.unwrap().index, 1);

    let board = svc
        .music_soundboard_save(SoundboardInput {
            id: None,
            name: "Board".into(),
            notes: None,
            buttons: vec![MusicSoundboardButton {
                id: String::new(),
                track_id: Some(b.id.clone()),
                label: "Drums".into(),
                icon: String::new(),
                color: String::new(),
                position: 0,
                loop_by_default: true,
                volume: 80,
                play_mode: "toggle".into(),
                hotkey: None,
            }],
        })
        .await
        .unwrap();
    let bid = board.buttons[0].id.clone();
    assert_eq!(
        svc.music_soundboard_press(bid.clone()).await.unwrap().toggles_playing,
        vec![bid.clone()]
    );
    assert!(svc.music_soundboard_press(bid.clone()).await.unwrap().toggles_playing.is_empty());
    let again = svc
        .music_soundboard_save(SoundboardInput {
            id: Some(board.id.clone()),
            name: "Board".into(),
            notes: None,
            buttons: board.buttons.clone(),
        })
        .await
        .unwrap();
    assert_eq!(again.buttons[0].id, bid, "button ids are stable");
}

// ── helpers ────────────────────────────────────────────────────────────

/// Collects engine commands until one matches `pred` (or `secs` elapse).
async fn wait_for(
    rx: &mut tokio::sync::broadcast::Receiver<Event>,
    secs: f64,
    pred: impl Fn(&Value) -> bool,
) -> (Vec<Value>, bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(secs);
    let mut out = vec![];
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(Event::MusicEngine { command })) => {
                let hit = pred(&command);
                out.push(command);
                if hit {
                    return (out, true);
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(_)) | Err(_) => return (out, false),
        }
    }
}

fn is(action: &'static str) -> impl Fn(&Value) -> bool {
    move |c| c["action"] == action
}

fn yt_input(title: &str, duration: Option<f64>) -> TrackInput {
    TrackInput {
        source: "youtube".into(),
        url: Some(format!("https://youtu.be/{title}")),
        file_path: None,
        title: title.into(),
        notes: None,
        loop_by_default: Some(true),
        duration,
        tags: vec![],
        category: None,
    }
}

fn is_invalid<T: std::fmt::Debug>(r: Result<T>) -> bool {
    matches!(r, Err(ServiceError::InvalidInput(_)))
}

// ── tracks ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn track_validation() {
    let (_d, svc) = temp_service();
    let ok = yt_input("ok", None);
    assert!(is_invalid(
        svc.music_track_create(TrackInput { title: " ".into(), ..ok.clone() }).await
    ));
    assert!(is_invalid(
        svc.music_track_create(TrackInput { url: Some("  ".into()), ..ok.clone() }).await
    ));
    assert!(is_invalid(
        svc.music_track_create(TrackInput { source: "spotify".into(), ..ok.clone() }).await
    ));
    assert!(is_invalid(
        svc.music_track_create(TrackInput { source: "upload".into(), url: None, ..ok.clone() })
            .await
    ));
    let t = svc.music_track_create(ok).await.unwrap();
    assert_eq!((t.title.as_str(), t.loop_by_default), ("ok", true));
}

#[tokio::test(flavor = "multi_thread")]
async fn uploads_live_in_the_blob_store() {
    let (d, svc) = temp_service();
    let f = d.path().join("drums.ogg");
    std::fs::write(&f, b"OggS fake audio").unwrap();
    let t = svc
        .music_track_create(TrackInput {
            source: "upload".into(),
            url: None,
            file_path: Some(f.clone()),
            ..yt_input("Drums", None)
        })
        .await
        .unwrap();
    let h = t.blob_hash.clone().unwrap();
    assert_eq!(svc.store().blob_meta(&h).unwrap().mime, "audio/ogg");
    // known issue 11: no absolute path is kept, so the source file can go away
    std::fs::remove_file(&f).unwrap();
    assert_eq!(svc.store().read_blob(&h).unwrap(), b"OggS fake audio");
    // editing without a new file keeps the blob
    let t2 = svc
        .music_track_update(
            t.id.clone(),
            TrackInput {
                source: "upload".into(),
                url: None,
                file_path: None,
                ..yt_input("Drums 2", None)
            },
        )
        .await
        .unwrap();
    assert_eq!(t2.blob_hash.as_deref(), Some(h.as_str()));
}

// ── live mix ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn channel_rules_volume_role_and_loop() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let t = svc.music_track_create(yt_input("hit", None)).await.unwrap();
    // the sfx role forces looping off, volumes clamp to 0..100
    let st = svc
        .music_channel_add(t.id.clone(), Some("sfx".into()), Some(150), Some(true), false)
        .await
        .unwrap();
    let ch = st.channels[0].clone();
    assert_eq!((ch.volume, ch.is_looping, ch.is_playing), (100, false, false));
    assert!(is_invalid(
        svc.music_channel_add(t.id.clone(), Some("lead".into()), None, None, true).await
    ));
    assert!(is_invalid(
        svc.music_channel_update(
            ch.id.clone(),
            ChannelPatch { role: Some("bass".into()), ..Default::default() }
        )
        .await
    ));
    assert!(matches!(
        svc.music_channel_update("nope".into(), ChannelPatch::default()).await,
        Err(ServiceError::NotFound(_))
    ));
    // a paused channel is not sent to the engine until it plays
    let mut rx = svc.subscribe();
    let st = svc
        .music_channel_update(
            ch.id.clone(),
            ChannelPatch { volume: Some(-5), is_playing: Some(true), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(st.channels[0].volume, 0);
    let cmds = engine_cmds(&mut rx);
    assert!(cmds.iter().any(|c| c["action"] == "play_track" && c["volume"] == 0), "{cmds:?}");
    // a live volume change ramps
    svc.music_channel_update(
        ch.id.clone(),
        ChannelPatch { volume: Some(40), ..Default::default() },
    )
    .await
    .unwrap();
    let cmds = engine_cmds(&mut rx);
    assert!(
        cmds.iter()
            .any(|c| c["action"] == "set_volume" && c["volume"] == 40 && c["ramp_ms"] == RAMP_MS),
        "{cmds:?}"
    );
    svc.music_channel_remove(ch.id.clone()).await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "stop_channel"));
}

#[tokio::test(flavor = "multi_thread")]
async fn play_all_pause_all_and_stop_all() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let a = svc.music_track_create(yt_input("a", Some(2.0))).await.unwrap();
    let b = svc.music_track_create(yt_input("b", Some(2.0))).await.unwrap();
    svc.music_channel_add(a.id.clone(), None, None, None, true).await.unwrap();
    svc.music_channel_add(b.id.clone(), None, None, None, true).await.unwrap();
    svc.music_soundbite_add(a.id.clone(), 300.0, 600.0, 50).await.unwrap();
    svc.music_set_duck(DuckMode::Strong).await.unwrap();
    svc.music_set_master(70).await.unwrap();
    let mut rx = svc.subscribe();

    let st = svc.music_set_all_playing(false).await.unwrap();
    assert!(st.channels.iter().all(|c| !c.is_playing) && !st.soundbites_running);
    assert_eq!(engine_cmds(&mut rx).iter().filter(|c| c["action"] == "pause_channel").count(), 2);
    let st = svc.music_set_all_playing(true).await.unwrap();
    assert!(st.channels.iter().all(|c| c.is_playing) && st.soundbites_running);
    assert_eq!(engine_cmds(&mut rx).iter().filter(|c| c["action"] == "resume_channel").count(), 2);

    let st = svc.music_stop_all().await.unwrap();
    assert!(st.channels.is_empty() && st.soundbites.is_empty() && st.soundscape_id.is_none());
    assert!(!st.soundbites_running && st.playlist.is_none());
    // duck mode and master volume survive a global stop
    assert_eq!((st.duck, st.master_volume), (DuckMode::Strong, 70));
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "stop_all"));
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_clears_stale_live_state() {
    // known issue 5: rows left playing by a crash are cleared on launch
    let (d, svc) = temp_service();
    let t = svc.music_track_create(yt_input("a", None)).await.unwrap();
    svc.music_channel_add(t.id.clone(), None, None, None, true).await.unwrap();
    svc.music_soundbite_add(t.id.clone(), 5.0, 9.0, 80).await.unwrap();
    drop(svc);
    let app = Service::open(d.path()).unwrap();
    app.music_attach_engine().await.unwrap();
    let st = app.music_state().await.unwrap();
    assert!(st.channels.is_empty() && st.soundbites.is_empty() && !st.soundbites_running);
    assert!(st.engine_attached);
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_events_update_the_mix() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let t = svc.music_track_create(yt_input("a", None)).await.unwrap();
    let st = svc.music_channel_add(t.id.clone(), None, None, Some(false), true).await.unwrap();
    let ch = st.channels[0].id.clone();
    // the engine reports the duration of tracks that had none
    svc.music_engine_event(EngineEvent::TrackDuration { channel_id: ch.clone(), seconds: 61.5 })
        .await
        .unwrap();
    assert_eq!(svc.music_track_get(t.id.clone()).await.unwrap().duration, Some(61.5));
    // a channel that fails (or ends without looping) stops playing
    svc.music_engine_event(EngineEvent::ChannelError { channel_id: ch.clone(), code: 150 })
        .await
        .unwrap();
    assert!(!svc.music_state().await.unwrap().channels[0].is_playing);
    // pressing play again restarts it in the engine
    let mut rx = svc.subscribe();
    svc.music_channel_update(
        ch.clone(),
        ChannelPatch { is_playing: Some(true), ..Default::default() },
    )
    .await
    .unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "play_track"));
    // a reloaded webview (`ready`) gets the whole mix again
    svc.music_engine_event(EngineEvent::Ready).await.unwrap();
    assert!(
        engine_cmds(&mut rx)
            .iter()
            .any(|c| c["action"] == "play_track" && c["channel_id"] == ch.as_str())
    );
}

// ── soundbites, SFX queue, ducking ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn soundbite_ducks_the_music_until_it_ends() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let music = svc.music_track_create(yt_input("music", None)).await.unwrap();
    let bell = svc.music_track_create(yt_input("bell", Some(20.0))).await.unwrap();
    svc.music_channel_add(music.id.clone(), Some("music".into()), Some(80), None, true)
        .await
        .unwrap();
    svc.music_set_duck(DuckMode::Soft).await.unwrap();
    let st = svc.music_soundbite_add(bell.id.clone(), 300.0, 600.0, 70).await.unwrap();
    let sb = st.soundbites[0].id.clone();
    let mut rx = svc.subscribe();

    svc.music_soundbite_play(sb.clone()).await.unwrap();
    let (cmds, ok) = wait_for(&mut rx, 3.0, is("play_oneshot")).await;
    assert!(ok, "{cmds:?}");
    // the music is lowered (80 × 0.5) before the clip starts
    assert!(cmds.iter().any(|c| c["action"] == "set_volume" && c["volume"] == 40), "{cmds:?}");
    let os = cmds.last().unwrap().clone();
    assert_eq!(os["volume"], 70);
    let st = svc.music_state().await.unwrap();
    assert!(st.ducked);
    assert_eq!(st.sfx_queue, 1);

    svc.music_engine_event(EngineEvent::OneshotEnded { osid: os["osid"].as_str().unwrap().into() })
        .await
        .unwrap();
    let (cmds, ok) =
        wait_for(&mut rx, 3.0, |c| c["action"] == "set_volume" && c["volume"] == 80).await;
    assert!(ok, "volume restored: {cmds:?}");
    let st = svc.music_state().await.unwrap();
    assert!(!st.ducked);
    assert_eq!(st.sfx_queue, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn duck_off_leaves_the_volume_alone() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let music = svc.music_track_create(yt_input("music", None)).await.unwrap();
    let bell = svc.music_track_create(yt_input("bell", Some(20.0))).await.unwrap();
    svc.music_channel_add(music.id.clone(), None, Some(80), None, true).await.unwrap();
    let st = svc.music_soundbite_add(bell.id.clone(), 300.0, 600.0, 70).await.unwrap();
    let mut rx = svc.subscribe();
    svc.music_soundbite_play(st.soundbites[0].id.clone()).await.unwrap();
    let (cmds, ok) = wait_for(&mut rx, 3.0, is("play_oneshot")).await;
    assert!(ok && !cmds.iter().any(|c| c["action"] == "set_volume"), "{cmds:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sfx_queue_is_capped_and_cleared_by_stop_all() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let bell = svc.music_track_create(yt_input("bell", Some(60.0))).await.unwrap();
    let st = svc.music_soundbite_add(bell.id.clone(), 300.0, 600.0, 70).await.unwrap();
    let sb = st.soundbites[0].id.clone();
    let mut rx = svc.subscribe();
    svc.music_soundbite_play(sb.clone()).await.unwrap();
    assert!(wait_for(&mut rx, 3.0, is("play_oneshot")).await.1);
    // one playing + 12 waiting; the rest are dropped (Python behaviour)
    for _ in 0..14 {
        svc.music_soundbite_play(sb.clone()).await.unwrap();
    }
    assert_eq!(svc.music_state().await.unwrap().sfx_queue, 1 + SFX_QUEUE_MAX);
    let st = svc.music_stop_all().await.unwrap();
    assert_eq!((st.sfx_queue, st.ducked), (0, false));
    // the stale queued clips never reach the engine
    let (cmds, hit) = wait_for(&mut rx, 0.6, is("play_oneshot")).await;
    assert!(!hit, "{cmds:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn soundbite_timers_fire_and_stop_with_pause_all() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let bell = svc.music_track_create(yt_input("bell", Some(0.2))).await.unwrap();
    // intervals clamp to ≥ 1 s and are reordered
    let st = svc.music_soundbite_add(bell.id.clone(), 0.0, 0.5, 60).await.unwrap();
    assert_eq!((st.soundbites[0].min_interval, st.soundbites[0].max_interval), (1.0, 1.0));
    assert!(st.soundbites_running);
    let mut rx = svc.subscribe();
    let (cmds, ok) = wait_for(&mut rx, 3.0, is("play_oneshot")).await;
    assert!(ok, "timer fired: {cmds:?}");
    let st = svc
        .music_soundbite_update(
            st.soundbites[0].id.clone(),
            SoundbitePatch {
                min_interval: Some(90.0),
                max_interval: Some(30.0),
                volume: Some(101),
            },
        )
        .await
        .unwrap();
    let b = &st.soundbites[0];
    assert_eq!((b.min_interval, b.max_interval, b.volume), (30.0, 90.0, 100));
    svc.music_set_all_playing(false).await.unwrap();
    let (cmds, hit) = wait_for(&mut rx, 1.5, is("play_oneshot")).await;
    assert!(!hit, "timers stopped: {cmds:?}");
}

// ── preview, video, standalone process ────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn preview_slot_and_video_toggle() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let t = svc.music_track_create(yt_input("a", None)).await.unwrap();
    let mut rx = svc.subscribe();
    svc.music_preview(t.id.clone(), Some(9999.0)).await.unwrap();
    let cmds = engine_cmds(&mut rx);
    assert!(
        cmds.iter().any(|c| c["action"] == "play_preview" && c["seconds"] == 600.0),
        "{cmds:?}"
    );
    assert!(svc.music_state().await.unwrap().previewing);
    svc.music_engine_event(EngineEvent::PreviewEnded).await.unwrap();
    assert!(!svc.music_state().await.unwrap().previewing);
    svc.music_preview(t.id.clone(), None).await.unwrap();
    svc.music_stop_preview().await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "stop_preview"));
    assert!(!svc.music_state().await.unwrap().previewing);
    svc.music_toggle_video().await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "toggle_video"));
}

#[tokio::test(flavor = "multi_thread")]
async fn standalone_process_queues_for_the_app() {
    // The MCP server has no engine: its changes are persisted and its
    // transient actions are queued; the app applies both on sync.
    let (d, app) = temp_service();
    app.music_attach_engine().await.unwrap();
    let mcp = Service::open(d.path()).unwrap();
    let t = mcp.music_track_create(yt_input("a", None)).await.unwrap();
    let st = mcp.music_channel_add(t.id.clone(), None, None, None, true).await.unwrap();
    assert!(!st.engine_attached);
    mcp.music_preview(t.id.clone(), Some(5.0)).await.unwrap();
    mcp.music_toggle_video().await.unwrap();
    let mut rx = app.subscribe();
    app.music_sync().await.unwrap();
    let cmds = engine_cmds(&mut rx);
    for a in ["play_track", "play_preview", "toggle_video"] {
        assert!(cmds.iter().any(|c| c["action"] == a), "{a}: {cmds:?}");
    }
    // the queue is drained once
    app.music_sync().await.unwrap();
    assert!(!engine_cmds(&mut rx).iter().any(|c| c["action"] == "play_preview"));
    // a global stop from the MCP side reaches the engine
    mcp.music_stop_all().await.unwrap();
    app.music_sync().await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "stop_all"));
}

// ── soundscapes ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn soundscapes_save_update_load_delete() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let a = svc.music_track_create(yt_input("a", None)).await.unwrap();
    let b = svc.music_track_create(yt_input("b", None)).await.unwrap();
    svc.music_channel_add(a.id.clone(), Some("ambient".into()), Some(55), None, true)
        .await
        .unwrap();
    svc.music_soundbite_add(b.id.clone(), 10.0, 20.0, 66).await.unwrap();
    assert!(is_invalid(
        svc.music_soundscape_save(SoundscapeInput { name: "  ".into(), notes: None, tags: vec![] })
            .await
    ));
    let sc = svc
        .music_soundscape_save(SoundscapeInput {
            name: "Cave".into(),
            notes: Some("drip".into()),
            tags: vec!["dark".into()],
        })
        .await
        .unwrap();
    assert_eq!((sc.channels.len(), sc.soundbites.len()), (1, 1));
    assert_eq!(svc.music_state().await.unwrap().soundscape_name.as_deref(), Some("Cave"));

    // update overwrites the mix and keeps the name, notes and tags
    svc.music_channel_add(b.id.clone(), None, None, None, true).await.unwrap();
    let sc2 = svc.music_soundscape_update(sc.id.clone()).await.unwrap();
    assert_eq!(
        (sc2.channels.len(), sc2.notes.as_str(), sc2.tags.clone()),
        (2, "drip", vec!["dark".to_string()])
    );

    svc.music_stop_all().await.unwrap();
    let st = svc.music_soundscape_load(sc.id.clone()).await.unwrap();
    assert_eq!(st.channels.len(), 2);
    assert_eq!(st.channels[0].volume, 55);
    assert!(st.soundbites_running && st.channels.iter().all(|c| c.is_playing));
    svc.music_soundscape_delete(sc.id.clone()).await.unwrap();
    assert!(svc.music_state().await.unwrap().soundscape_id.is_none());
    assert!(svc.music_soundscapes_list().await.unwrap().is_empty());
}

// ── playlists ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn playlist_controls() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let mut ids = vec![];
    for n in ["one", "two", "three"] {
        ids.push(svc.music_track_create(yt_input(n, None)).await.unwrap().id);
    }
    let input = |id: Option<String>, name: &str, track_ids| PlaylistInput {
        id,
        name: name.into(),
        track_ids,
    };
    assert!(is_invalid(svc.music_playlist_save(input(None, "", ids.clone())).await));
    let empty = svc.music_playlist_save(input(None, "Empty", vec![])).await.unwrap();
    assert!(is_invalid(svc.music_playlist_play(empty.id.clone(), None).await));
    let pl = svc.music_playlist_save(input(None, "Set", ids.clone())).await.unwrap();
    // replacing keeps the id
    let pl = svc.music_playlist_save(input(Some(pl.id.clone()), "Set", ids.clone())).await.unwrap();

    let mut rx = svc.subscribe();
    let st = svc.music_playlist_play(pl.id.clone(), Some(99)).await.unwrap();
    assert_eq!(st.playlist.as_ref().unwrap().index, 2, "index clamps to the last track");
    let cmds = engine_cmds(&mut rx);
    assert!(cmds.iter().any(|c| c["action"] == "play_track"
        && c["channel_id"] == "playlist"
        && c["loop"] == false
        && c["volume"] == 80));
    let st = svc.music_playlist_control("next".into()).await.unwrap();
    assert_eq!(st.playlist.as_ref().unwrap().index, 2);
    let st = svc.music_playlist_control("prev".into()).await.unwrap();
    let p = st.playlist.unwrap();
    assert_eq!((p.index, p.title.as_str()), (1, "two"));
    let cmds = engine_cmds(&mut rx);
    assert!(cmds.iter().any(|c| c["action"] == "stop_channel"));
    assert!(cmds.iter().any(|c| c["action"] == "play_track" && c["src"] == "https://youtu.be/two"));
    svc.music_playlist_control("pause".into()).await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "pause_channel"));
    svc.music_playlist_control("resume".into()).await.unwrap();
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "resume_channel"));
    assert!(is_invalid(svc.music_playlist_control("shuffle".into()).await));
    // the last track ending finishes the playlist
    svc.music_playlist_control("next".into()).await.unwrap();
    svc.music_engine_event(EngineEvent::ChannelEnded { channel_id: "playlist".into() })
        .await
        .unwrap();
    assert!(svc.music_state().await.unwrap().playlist.is_none());
    svc.music_playlist_play(pl.id.clone(), None).await.unwrap();
    engine_cmds(&mut rx);
    let st = svc.music_playlist_control("stop".into()).await.unwrap();
    assert!(st.playlist.is_none());
    assert!(
        engine_cmds(&mut rx)
            .iter()
            .any(|c| c["action"] == "stop_channel" && c["channel_id"] == "playlist")
    );
    // deleting the playing playlist stops it
    svc.music_playlist_play(pl.id.clone(), None).await.unwrap();
    svc.music_playlist_delete(pl.id.clone()).await.unwrap();
    assert!(svc.music_state().await.unwrap().playlist.is_none());
}

// ── soundboards ────────────────────────────────────────────────────────

fn button(track: &str, mode: &str, looping: bool) -> MusicSoundboardButton {
    MusicSoundboardButton {
        id: String::new(),
        track_id: Some(track.into()),
        label: mode.into(),
        icon: String::new(),
        color: String::new(),
        position: 0,
        loop_by_default: looping,
        volume: 90,
        play_mode: mode.into(),
        hotkey: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn soundboard_modes() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let t = svc.music_track_create(yt_input("boom", Some(1.0))).await.unwrap();
    let bad = |mode: &str, track: Option<String>| SoundboardInput {
        id: None,
        name: "B".into(),
        notes: None,
        buttons: vec![MusicSoundboardButton { track_id: track, ..button(&t.id, mode, false) }],
    };
    assert!(is_invalid(svc.music_soundboard_save(bad("oneshot", None)).await));
    assert!(is_invalid(svc.music_soundboard_save(bad("hold", Some(t.id.clone()))).await));
    let board = svc
        .music_soundboard_save(SoundboardInput {
            id: None,
            name: "Board".into(),
            notes: None,
            buttons: vec![button(&t.id, "oneshot", false), button(&t.id, "restart", false)],
        })
        .await
        .unwrap();
    let (os, rs) = (board.buttons[0].id.clone(), board.buttons[1].id.clone());
    let mut rx = svc.subscribe();

    // one-shot: plays at once (no queue) and ducks the music until it ends
    let st = svc.music_soundboard_press(os.clone()).await.unwrap();
    assert!(st.toggles_playing.is_empty() && st.sfx_queue == 0 && st.ducked);
    let os1 = engine_cmds(&mut rx)
        .into_iter()
        .find(|c| c["action"] == "play_oneshot" && c["volume"] == 90)
        .unwrap();
    svc.music_engine_event(EngineEvent::OneshotEnded {
        osid: os1["osid"].as_str().unwrap().into(),
    })
    .await
    .unwrap();
    assert!(!svc.music_state().await.unwrap().ducked);

    // restart: plays on the button's channel; pressing again starts it over
    svc.music_soundboard_press(rs.clone()).await.unwrap();
    assert!(
        engine_cmds(&mut rx)
            .iter()
            .any(|c| c["action"] == "play_track" && c["channel_id"] == rs.as_str())
    );
    svc.music_soundboard_press(rs.clone()).await.unwrap();
    let cmds = engine_cmds(&mut rx);
    let stop =
        cmds.iter().position(|c| c["action"] == "stop_channel" && c["channel_id"] == rs.as_str());
    let play =
        cmds.iter().position(|c| c["action"] == "play_track" && c["channel_id"] == rs.as_str());
    assert!(matches!((stop, play), (Some(a), Some(b)) if a < b), "{cmds:?}");
    // once it ends it is no longer part of the desired state
    svc.music_engine_event(EngineEvent::ChannelEnded { channel_id: rs.clone() }).await.unwrap();
    svc.music_engine_event(EngineEvent::Ready).await.unwrap();
    assert!(
        !engine_cmds(&mut rx)
            .iter()
            .any(|c| c["channel_id"] == rs.as_str() && c["action"] == "play_track")
    );
    svc.music_soundboard_delete(board.id).await.unwrap();
    assert!(svc.music_soundboards_list().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn soundboard_oneshots_duck_without_queueing() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let music = svc.music_track_create(yt_input("music", None)).await.unwrap();
    let boom = svc.music_track_create(yt_input("boom", Some(20.0))).await.unwrap();
    svc.music_channel_add(music.id.clone(), None, Some(80), None, true).await.unwrap();
    svc.music_set_duck(DuckMode::Strong).await.unwrap();
    let board = svc
        .music_soundboard_save(SoundboardInput {
            id: None,
            name: "FX".into(),
            notes: None,
            buttons: vec![button(&boom.id, "oneshot", false)],
        })
        .await
        .unwrap();
    let b = board.buttons[0].id.clone();
    let mut rx = svc.subscribe();

    // the music drops (80 × 0.25) before the clip starts
    svc.music_soundboard_press(b.clone()).await.unwrap();
    let cmds = engine_cmds(&mut rx);
    let duck = cmds.iter().position(|c| c["action"] == "set_volume" && c["volume"] == 20);
    let play = cmds.iter().position(|c| c["action"] == "play_oneshot");
    assert!(matches!((duck, play), (Some(a), Some(p)) if a < p), "{cmds:?}");
    // a second press overlaps instead of queueing, and does not re-duck
    svc.music_soundboard_press(b.clone()).await.unwrap();
    let cmds2 = engine_cmds(&mut rx);
    assert!(cmds2.iter().any(|c| c["action"] == "play_oneshot"), "{cmds2:?}");
    assert!(!cmds2.iter().any(|c| c["action"] == "set_volume"), "{cmds2:?}");
    assert_eq!(svc.music_state().await.unwrap().sfx_queue, 0);

    let osid = |c: &Value| EngineEvent::OneshotEnded { osid: c["osid"].as_str().unwrap().into() };
    let first = cmds.iter().find(|c| c["action"] == "play_oneshot").unwrap();
    let second = cmds2.iter().find(|c| c["action"] == "play_oneshot").unwrap();
    // still ducked while the second clip plays
    svc.music_engine_event(osid(first)).await.unwrap();
    assert!(svc.music_state().await.unwrap().ducked);
    assert!(!engine_cmds(&mut rx).iter().any(|c| c["action"] == "set_volume"));
    svc.music_engine_event(osid(second)).await.unwrap();
    assert!(!svc.music_state().await.unwrap().ducked);
    assert!(engine_cmds(&mut rx).iter().any(|c| c["action"] == "set_volume" && c["volume"] == 80));

    // a global stop clears it too
    svc.music_soundboard_press(b).await.unwrap();
    assert!(svc.music_state().await.unwrap().ducked);
    assert!(!svc.music_stop_all().await.unwrap().ducked);
}

// ── .dmix files ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn dmix_embeds_uploads_and_carries_settings() {
    let (d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let f = d.path().join("wind.mp3");
    std::fs::write(&f, b"ID3 wind").unwrap();
    let up = svc
        .music_track_create(TrackInput {
            source: "upload".into(),
            url: None,
            file_path: Some(f),
            ..yt_input("Wind", Some(12.0))
        })
        .await
        .unwrap();
    let yt = svc.music_track_create(yt_input("rain", None)).await.unwrap();
    // an empty mix cannot be exported
    assert!(is_invalid(svc.music_mix_export(d.path().join("x.dmix"), None).await));
    svc.music_channel_add(up.id.clone(), Some("ambient".into()), Some(33), Some(false), true)
        .await
        .unwrap();
    svc.music_channel_add(yt.id.clone(), Some("music".into()), Some(77), None, true).await.unwrap();
    svc.music_soundbite_add(up.id.clone(), 12.0, 24.0, 44).await.unwrap();
    svc.music_set_master(60).await.unwrap();
    svc.music_set_duck(DuckMode::Strong).await.unwrap();
    let path = d.path().join("Windy pass.dmix");
    svc.music_mix_export(path.clone(), None).await.unwrap();
    let doc: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!((doc["format"].as_str(), doc["version"].as_i64()), (Some("sleuth-dmix"), Some(1)));
    assert_eq!(doc["ducking"]["factor"], 0.25);

    // a fresh install recreates the tracks and the upload bytes
    let (_d2, other) = temp_service();
    other.music_attach_engine().await.unwrap();
    let r = other.music_mix_import(path.clone(), false).await.unwrap();
    assert_eq!((r.name.as_str(), r.tracks_created, r.tracks_reused), ("Windy pass", 2, 1));
    let st = &r.state;
    assert_eq!((st.master_volume, st.duck), (60, DuckMode::Strong));
    assert_eq!(st.channels.iter().map(|c| c.volume).collect::<Vec<_>>(), vec![33, 77]);
    assert!(st.channels.iter().all(|c| !c.is_playing), "play=false loads paused");
    assert!(!st.channels[0].is_looping);
    let t = other.music_track_get(st.channels[0].track_id.clone()).await.unwrap();
    assert_eq!(other.store().read_blob(t.blob_hash.as_deref().unwrap()).unwrap(), b"ID3 wind");
    // importing again reuses everything and overwrites the same soundscape
    let r2 = other.music_mix_import(path, true).await.unwrap();
    assert_eq!((r2.tracks_created, r2.soundscape_id.as_str()), (0, r.soundscape_id.as_str()));
    assert_eq!(other.music_soundscapes_list().await.unwrap().len(), 1);

    // rejects other files and newer versions
    let bad = d.path().join("bad.dmix");
    std::fs::write(&bad, b"not json").unwrap();
    assert!(is_invalid(other.music_mix_import(bad.clone(), false).await));
    std::fs::write(&bad, br#"{"format":"other"}"#).unwrap();
    assert!(is_invalid(other.music_mix_import(bad.clone(), false).await));
    std::fs::write(&bad, br#"{"format":"sleuth-dmix","version":2}"#).unwrap();
    assert!(is_invalid(other.music_mix_import(bad, false).await));
}

// ── legacy import ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn legacy_music_import_is_idempotent() {
    let (d, svc) = temp_service();
    assert!(is_invalid(svc.music_import_legacy().await));
    // campaigns are hidden now, but legacy links are still preserved
    let c = svc.store().create_campaign("Barovia").unwrap().id;
    let local = d.path().join("old.wav");
    std::fs::write(&local, b"RIFF old").unwrap();
    let dump = json!({
        "music_tracks": [
            { "id": "t1", "url": "https://youtu.be/abc", "title": "Tavern", "campaign_id": c,
              "loop_by_default": 1, "duration": 90.0 },
            { "id": "t2", "url": "local:Z:/gone/x.mp3", "title": "Missing" },
            { "id": "t3", "url": format!("local:{}", local.display()), "title": "Old",
              "campaign_id": "deleted-campaign", "loop_by_default": 0 },
        ],
        "music_tags": [{ "id": "g1", "name": "calm" }],
        "music_track_tags": [{ "track_id": "t1", "tag_id": "g1" }],
        "music_soundscapes": [{ "id": "s1", "name": "Inn", "campaign_id": "deleted-campaign", "notes": "" }],
        "music_soundscape_channels": [
            { "soundscape_id": "s1", "track_id": "t1", "volume": 70, "is_looping": 1, "role": "ambient" },
            { "soundscape_id": "s1", "track_id": "t2", "volume": 70, "is_looping": 1, "role": "ambient" },
        ],
        "music_playlists": [{ "id": "p1", "name": "Night", "campaign_id": c }],
        "music_playlist_tracks": [
            { "playlist_id": "p1", "track_id": "t3", "position": 1 },
            { "playlist_id": "p1", "track_id": "t1", "position": 0 },
        ],
        "music_soundboards": [{ "id": "b1", "name": "FX" }],
        "music_soundboard_buttons": [
            { "soundboard_id": "b1", "track_id": "t1", "label": "Cheer", "position": 0, "play_mode": "" },
        ],
    });
    std::fs::create_dir_all(d.path().join("legacy")).unwrap();
    std::fs::write(d.path().join("legacy").join("music.json"), dump.to_string()).unwrap();

    let r = svc.music_import_legacy().await.unwrap();
    assert_eq!((r.tracks, r.soundscapes, r.playlists, r.soundboards), (2, 1, 1, 1));
    assert_eq!(r.skipped.len(), 1, "{:?}", r.skipped);
    let tracks = svc.music_tracks_list(TrackQuery::default()).await.unwrap();
    let tavern = tracks.iter().find(|t| t.title == "Tavern").unwrap();
    assert_eq!(tavern.tags, vec!["calm".to_string()]);
    assert_eq!(tavern.campaign_ids, vec![c.clone()]);
    let old = tracks.iter().find(|t| t.title == "Old").unwrap();
    assert_eq!((old.source.as_str(), old.loop_by_default), ("upload", false));
    assert!(old.campaign_ids.is_empty(), "unknown campaigns are dropped");
    let sc = &svc.music_soundscapes_list().await.unwrap()[0];
    assert_eq!((sc.campaign_id.as_deref(), sc.channels.len()), (None, 1));
    let pl = &svc.music_playlists_list().await.unwrap()[0];
    assert_eq!(
        pl.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        vec!["Tavern", "Old"]
    );
    let sb = &svc.music_soundboards_list().await.unwrap()[0];
    assert_eq!(sb.buttons[0].play_mode, "oneshot");

    let again = svc.music_import_legacy().await.unwrap();
    assert_eq!((again.tracks, again.soundscapes, again.playlists, again.soundboards), (0, 0, 0, 0));
    assert_eq!(svc.music_tracks_list(TrackQuery::default()).await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn tracks_filter_by_favourite_loop_and_length() {
    let (_d, svc) = temp_service();
    let mk = |title: &str, looping: bool, duration: Option<f64>| TrackInput {
        source: "youtube".into(),
        url: Some(format!("https://youtu.be/{title}")),
        file_path: None,
        title: title.into(),
        notes: None,
        loop_by_default: Some(looping),
        duration,
        tags: vec![],
        category: None,
    };
    let rain = svc.music_track_create(mk("Rain", true, Some(600.0))).await.unwrap();
    let door = svc.music_track_create(mk("Door", false, Some(3.0))).await.unwrap();
    svc.music_track_create(mk("Unknown", true, None)).await.unwrap();
    assert!(!rain.favorite);

    let fav = svc.music_track_set_favorite(rain.id.clone(), true).await.unwrap();
    assert!(fav.favorite);
    let titles = |v: Vec<Track>| v.into_iter().map(|t| t.title).collect::<Vec<_>>();
    let q = |f: fn(&mut TrackQuery)| {
        let mut q = TrackQuery::default();
        f(&mut q);
        q
    };
    assert_eq!(
        titles(svc.music_tracks_list(q(|q| q.favorites_only = true)).await.unwrap()),
        ["Rain"]
    );
    assert_eq!(
        titles(svc.music_tracks_list(q(|q| q.looping = Some(false))).await.unwrap()),
        ["Door"]
    );
    // Length bounds leave out tracks of unknown length.
    assert_eq!(
        titles(svc.music_tracks_list(q(|q| q.max_duration = Some(60.0))).await.unwrap()),
        ["Door"]
    );
    assert_eq!(
        titles(svc.music_tracks_list(q(|q| q.min_duration = Some(300.0))).await.unwrap()),
        ["Rain"]
    );
    assert_eq!(svc.music_tracks_list(TrackQuery::default()).await.unwrap().len(), 3);

    // Editing a track keeps it a favourite.
    let mut input = mk("Rain (edited)", true, Some(600.0));
    input.tags = vec!["weather".into()];
    let edited = svc.music_track_update(rain.id.clone(), input).await.unwrap();
    assert!(edited.favorite);
    assert!(svc.music_track_set_favorite("nope".into(), true).await.is_err());
    let _ = door;
}

#[tokio::test(flavor = "multi_thread")]
async fn pinned_soundscapes_come_first_and_survive_updates() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let a = svc.music_track_create(yt_input("a", None)).await.unwrap();
    svc.music_channel_add(a.id.clone(), None, None, None, false).await.unwrap();
    let save = |name: &str| SoundscapeInput { name: name.into(), notes: None, tags: vec![] };
    svc.music_soundscape_save(save("Alpha")).await.unwrap();
    let zulu = svc.music_soundscape_save(save("Zulu")).await.unwrap();
    assert!(!zulu.pinned);

    let names = |v: Vec<MusicSoundscape>| v.into_iter().map(|s| s.name).collect::<Vec<_>>();
    assert_eq!(names(svc.music_soundscapes_list().await.unwrap()), ["Alpha", "Zulu"]);
    assert!(svc.music_soundscape_set_pinned(zulu.id.clone(), true).await.unwrap().pinned);
    assert_eq!(names(svc.music_soundscapes_list().await.unwrap()), ["Zulu", "Alpha"]);

    // Overwriting the mix keeps the pin.
    let updated = svc.music_soundscape_update(zulu.id.clone()).await.unwrap();
    assert!(updated.pinned);
    svc.music_soundscape_set_pinned(zulu.id.clone(), false).await.unwrap();
    assert_eq!(names(svc.music_soundscapes_list().await.unwrap()), ["Alpha", "Zulu"]);
    assert!(svc.music_soundscape_set_pinned("nope".into(), true).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn categories_are_guessed_kept_and_route_the_mixer() {
    let (_d, svc) = temp_service();
    svc.music_attach_engine().await.unwrap();
    let mk = |title: &str, looping: bool, duration: f64, category: Option<&str>| TrackInput {
        source: "youtube".into(),
        url: Some(format!("https://youtu.be/{title}")),
        file_path: None,
        title: title.into(),
        notes: None,
        loop_by_default: Some(looping),
        duration: Some(duration),
        tags: vec![],
        category: category.map(String::from),
    };
    let amb = svc.music_track_create(mk("Swamp Ambience", true, 3600.0, None)).await.unwrap();
    let sfx = svc.music_track_create(mk("SFX - Rat Screech", false, 16.0, None)).await.unwrap();
    let song = svc.music_track_create(mk("Tavern", true, 200.0, Some("music"))).await.unwrap();
    assert_eq!(
        (amb.category.as_str(), sfx.category.as_str(), song.category.as_str()),
        ("ambient", "soundbite", "music")
    );
    assert!(svc.music_track_create(mk("Bad", true, 1.0, Some("sfx"))).await.is_err());

    // An update without a category keeps it; with one, changes it.
    let kept = svc
        .music_track_update(amb.id.clone(), mk("Swamp at night", true, 3600.0, None))
        .await
        .unwrap();
    assert_eq!(kept.category, "ambient");
    let moved = svc
        .music_track_update(song.id.clone(), mk("Tavern", true, 200.0, Some("ambient")))
        .await
        .unwrap();
    assert_eq!(moved.category, "ambient");

    // The category filter.
    let q = TrackQuery { category: Some("soundbite".into()), ..Default::default() };
    assert_eq!(svc.music_tracks_list(q).await.unwrap().len(), 1);

    // Without an explicit role, channels take the category's role.
    let st = svc.music_channel_add(amb.id.clone(), None, None, None, false).await.unwrap();
    assert_eq!(st.channels.last().unwrap().role, "ambient");
    let st = svc.music_channel_add(sfx.id.clone(), None, None, None, false).await.unwrap();
    assert_eq!(st.channels.last().unwrap().role, "sfx");
    let st = svc
        .music_channel_add(amb.id.clone(), Some("music".into()), None, None, false)
        .await
        .unwrap();
    assert_eq!(st.channels.last().unwrap().role, "music");
}

#[tokio::test(flavor = "multi_thread")]
async fn tag_suggestions_add_but_never_remove() {
    let (_d, svc) = temp_service();
    let mut input = yt_input("Dark Swamp | Exploration Ambience", None);
    input.tags = vec!["Swamp".into(), "mine".into()];
    let t = svc.music_track_create(input).await.unwrap();
    svc.music_track_create(yt_input("Plain", None)).await.unwrap();

    let sug = svc.music_tags_suggest().await.unwrap();
    assert_eq!(sug.len(), 1, "only tracks with something to add");
    assert_eq!(sug[0].add, ["exploration", "dark"], "existing tags (any case) are skipped");
    assert_eq!(svc.music_tags_apply(sug).await.unwrap(), 1);
    let after = svc.music_track_get(t.id.clone()).await.unwrap();
    let mut tags = after.tags.clone();
    tags.sort();
    assert_eq!(tags, ["Swamp", "dark", "exploration", "mine"]);
    assert_eq!(after.category, "ambient");
    assert!(svc.music_tags_suggest().await.unwrap().is_empty());
}
