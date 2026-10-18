#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use genos_core::{
    blank::{self, BlankOptions},
    capture::{self, CaptureSnapshot, MidiCapture},
    chord_preview,
    document::View,
    edit::{Command, Session},
    effect_catalog, exchange, expansion_guitar, export_csv, export_json, genos_system_effect,
    midi::{self, InputMonitor, Transport},
    pitch_shift_preview,
    prefs::{self, Prefs},
    project,
    recent::{self, RecentEntryView},
    voice_catalog::{self, Address},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Manager, State};

#[derive(Default)]
struct Studio {
    session: Option<Session>,
    transport: Transport,
    monitor: InputMonitor,
    capture: MidiCapture,
    epoch: u64,
    exchange_nonce: u64,
    pending_exchange: Option<PendingExchange>,
    pending_live: Option<PendingLive>,
}
struct PendingLive {
    token: String,
    epoch: u64,
    revision: u64,
    compiled: genos_core::live_preview::Compiled,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LivePreview {
    token: String,
    report: genos_core::live_preview::Report,
}
#[tauri::command]
fn preview_live_style(
    config: genos_core::live_preview::Config,
    revision: u64,
    state: State<'_, Shared>,
) -> Result<LivePreview, String> {
    let mut s = state.lock().map_err(error)?;
    s.pending_live = None;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if session.revision != revision {
        return Err("Document changed; check the preview again".into());
    }
    let compiled = genos_core::live_preview::compile(&session.document, &config).map_err(error)?;
    s.exchange_nonce += 1;
    let token = format!("live:{}:{}:{}", s.epoch, revision, s.exchange_nonce);
    let report = compiled.report.clone();
    s.pending_live = Some(PendingLive {
        token: token.clone(),
        epoch: s.epoch,
        revision,
        compiled,
    });
    Ok(LivePreview { token, report })
}
#[tauri::command]
fn play_live_style(
    token: String,
    index: usize,
    name: String,
    looping: bool,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    let pending = s
        .pending_live
        .as_ref()
        .ok_or("Check live preview before Play")?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if pending.token != token || pending.epoch != s.epoch || pending.revision != session.revision {
        return Err("Document or preview changed; check live preview again".into());
    }
    if s.capture.is_recording() {
        return Err("Stop capture before live preview".into());
    }
    let plan = pending.compiled.plan.clone();
    let channels = pending.compiled.channels.clone();
    s.transport
        .play_plan(plan, channels, index, name, looping)
        .map_err(error)
}
struct PendingExchange {
    token: String,
    epoch: u64,
    revision: u64,
    section: u64,
    channel: u8,
    notes: Vec<exchange::PhraseNote>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangePreview {
    token: String,
    #[serde(flatten)]
    preview: exchange::Preview,
}
#[derive(serde::Serialize)]
struct EffectPreview {
    token: String,
    #[serde(flatten)]
    setup: genos_core::style_effects::PreviewSetup,
}
type Shared = Arc<Mutex<Studio>>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn app_data(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(error)
}
fn recovery_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data(app)?.join("recovery.gss"))
}
fn prefs_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data(app)?.join("prefs.json"))
}
fn catalog_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data(app)?.join("voice-catalog.json"))
}
fn effect_catalog_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data(app)?.join("effect-catalog.json"))
}
#[tauri::command]
fn voice_catalog_status(app: tauri::AppHandle) -> Result<Option<voice_catalog::Summary>, String> {
    Ok(voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .map(|c| c.summary()))
}
#[tauri::command]
async fn import_voice_catalog(
    path: String,
    app: tauri::AppHandle,
) -> Result<voice_catalog::Summary, String> {
    let destination = catalog_file(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let file = std::fs::File::open(&path).map_err(error)?;
        if file.metadata().map_err(error)?.len() > voice_catalog::MAX_PDF_BYTES as u64 {
            return Err("Data List exceeds 32 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(voice_catalog::MAX_PDF_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        let name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let catalog = voice_catalog::import_pdf(&bytes, &name).map_err(error)?;
        voice_catalog::save(&destination, &catalog).map_err(error)?;
        Ok(catalog.summary())
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn import_effect_catalog(
    path: String,
    app: tauri::AppHandle,
) -> Result<effect_catalog::Summary, String> {
    let destination = effect_catalog_file(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let file = std::fs::File::open(&path).map_err(error)?;
        if file.metadata().map_err(error)?.len() > voice_catalog::MAX_PDF_BYTES as u64 {
            return Err("Data List exceeds 32 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(voice_catalog::MAX_PDF_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        let name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let catalog = effect_catalog::import_pdf(&bytes, &name).map_err(error)?;
        effect_catalog::save(&destination, &catalog).map_err(error)?;
        Ok(catalog.summary())
    })
    .await
    .map_err(error)?
}
#[tauri::command]
fn effect_catalog_status(app: tauri::AppHandle) -> Result<Option<effect_catalog::Summary>, String> {
    Ok(effect_catalog::load(&effect_catalog_file(&app)?)
        .map_err(error)?
        .map(|c| c.summary()))
}
#[tauri::command]
fn search_effect_catalog(
    query: String,
    block: Option<String>,
    app: tauri::AppHandle,
) -> Result<effect_catalog::Search, String> {
    effect_catalog::load(&effect_catalog_file(&app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List first")?
        .search(&query, block.as_deref())
        .map_err(error)
}
#[tauri::command]
fn search_voice_catalog(
    query: String,
    app: tauri::AppHandle,
) -> Result<voice_catalog::Search, String> {
    voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List first")?
        .search(&query)
        .map_err(error)
}
#[tauri::command]
fn search_drum_kits(query: String, app: tauri::AppHandle) -> Result<voice_catalog::Search, String> {
    voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List first")?
        .search_drum_kits(&query)
        .map_err(error)
}
#[tauri::command]
fn drum_channel_status(
    section: u64,
    revision: u64,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<Vec<voice_catalog::DrumChannelStatus>, String> {
    let catalog = voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List first")?;
    let s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if session.revision != revision {
        return Err("Document changed; refresh drum kit status".into());
    }
    voice_catalog::drum_channel_status(&session.document, &catalog, section).map_err(error)
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn preview_drum_note(
    index: usize,
    name: String,
    channel: Option<u8>,
    pitch: Option<u8>,
    velocity: Option<u8>,
    hold_ms: Option<u64>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before auditioning a drum note".into());
    }
    midi::preview_drum_note(
        index,
        &name,
        channel.unwrap_or(10),
        pitch.unwrap_or(36),
        velocity.unwrap_or(90),
        hold_ms.unwrap_or(200),
    )
    .map_err(error)
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn apply_genos_system_effect(
    index: usize,
    name: String,
    block: String,
    msb: Option<u8>,
    lsb: Option<u8>,
    return_level: Option<u8>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<genos_system_effect::ApplyResult, String> {
    let s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before changing system effects".into());
    }
    if let (Some(msb), Some(lsb)) = (msb, lsb) {
        let catalog = effect_catalog::load(&effect_catalog_file(&app)?)
            .map_err(error)?
            .ok_or("Import the Genos effect Data List first")?;
        if !catalog
            .effects
            .iter()
            .any(|e| e.block == block && e.msb == msb && e.lsb == lsb)
        {
            return Err("Effect type is not in the imported catalog".into());
        }
    }
    genos_system_effect::apply_system_effect(index, &name, &block, msb, lsb, return_level)
        .map_err(error)
}
#[tauri::command]
fn chord_preview_vocabulary() -> chord_preview::Vocabulary {
    chord_preview::vocabulary()
}
#[tauri::command]
fn inspect_chord_preview(root: u8, quality: String) -> Result<chord_preview::ChordInspect, String> {
    chord_preview::inspect(root, &quality).map_err(error)
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn remap_chord_preview_notes(
    notes: Vec<u8>,
    source_root: Option<u8>,
    source_quality: Option<String>,
    target_root: u8,
    target_quality: String,
    ntt: Option<String>,
    ntr: Option<String>,
    high_key: Option<u8>,
    kind: Option<String>,
    bass: Option<i8>,
    expansion_layout: Option<bool>,
) -> Result<Vec<chord_preview::RemappedNote>, String> {
    let source_quality = source_quality.unwrap_or_else(|| "Major".into());
    let ntt = ntt.unwrap_or_else(|| "chord".into());
    let ntr = ntr.unwrap_or_else(|| "root_trans".into());
    let kind = kind.unwrap_or_else(|| "pitched".into());
    let note_limit_high = if expansion_layout.unwrap_or(false) || kind == "guitar" {
        95
    } else {
        127
    };
    chord_preview::remap_notes_with_rule(
        &notes,
        &chord_preview::PitchRule {
            source_root: source_root.unwrap_or(0),
            source_quality: &source_quality,
            target_root,
            target_quality: &target_quality,
            ntr: &ntr,
            ntt: &ntt,
            high_key: Some(high_key.unwrap_or(11)),
            note_limit_low: 0,
            note_limit_high,
            kind: &kind,
            bass: bass.unwrap_or(-1),
            expansion_layout: expansion_layout.unwrap_or(false),
        },
    )
    .map_err(error)
}
#[tauri::command]
fn recognize_chord_preview(
    notes: Vec<u8>,
) -> Result<Option<chord_preview::RecognizedChord>, String> {
    Ok(chord_preview::recognize(&notes, None))
}
#[tauri::command]
fn pitch_shift_bend_message(
    channel: u8,
    offset: i8,
    sensitivity: u8,
) -> Result<pitch_shift_preview::BendMessage, String> {
    pitch_shift_preview::bend_message(channel, offset, sensitivity).map_err(error)
}
#[tauri::command]
fn pitch_shift_wire_pitch(channel: u8, target_pitch: u8, offset: i8) -> Result<Option<u8>, String> {
    let mut overlay = pitch_shift_preview::PitchShiftOverlay::default();
    overlay.set_offset(channel, offset);
    Ok(overlay.wire_pitch(channel, target_pitch))
}
#[tauri::command]
fn pitch_shift_retarget(
    channel: u8,
    sensitivity: Option<u8>,
    slots: Vec<pitch_shift_preview::PitchShiftSlot>,
    converted: Option<bool>,
) -> Result<pitch_shift_preview::RetargetResult, String> {
    let mut ranges = std::collections::BTreeMap::new();
    if let Some(s) = sensitivity {
        ranges.insert(channel, s);
    }
    let mut overlay = pitch_shift_preview::PitchShiftOverlay::new(ranges);
    overlay
        .retarget(channel, &slots, converted.unwrap_or(true))
        .map_err(error)
}
#[tauri::command]
fn validate_expansion_guitar_profiles(
    profiles: Vec<expansion_guitar::ExpansionGuitarProfile>,
) -> Result<Vec<expansion_guitar::ExpansionGuitarProfile>, String> {
    expansion_guitar::validate_expansion_guitar_profiles(&profiles).map_err(error)
}

#[tauri::command]
fn analyze_expansion_pack(
    ppi_path: Option<String>,
    n27_path: Option<String>,
    ppf_path: Option<String>,
    wave_index_path: Option<String>,
    encryption_key_hex: Option<String>,
) -> Result<genos_core::expansion::RecoveryReport, String> {
    use genos_core::expansion::recovery::RecoverOptions;
    genos_core::expansion::recovery::recover_pack_with_options(
        ppi_path.as_deref().map(Path::new),
        n27_path.as_deref().map(Path::new),
        ppf_path.as_deref().map(Path::new),
        RecoverOptions {
            wave_index_path: wave_index_path.map(PathBuf::from),
            encryption_key_hex,
        },
    )
    .map_err(error)
}

#[tauri::command]
fn extract_expansion_resource(
    request: genos_core::expansion::ExtractRequest,
) -> Result<String, String> {
    genos_core::expansion::recovery::extract_resource(&request)
        .map(|p| p.display().to_string())
        .map_err(error)
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn expansion_guitar_candidate_phase1(
    msb: u8,
    lsb: u8,
    program: u8,
    is_drum_kit: Option<bool>,
    factory_known: Option<bool>,
    imported_from_casm: Option<bool>,
    casm_edited: Option<bool>,
    shared_source_voice: Option<bool>,
    changing_voice: Option<bool>,
    tick_zero_converges: Option<bool>,
    future_voice_stable: Option<bool>,
) -> Result<Option<expansion_guitar::ExpansionGuitarProfile>, String> {
    let address = expansion_guitar::parse_voice_address(msb, lsb, program)
        .ok_or_else(|| "Invalid expansion Guitar preview address".to_string())?;
    Ok(expansion_guitar::expansion_guitar_candidate_phase1(
        &expansion_guitar::ExpansionCandidateGates {
            address,
            is_drum_kit: is_drum_kit.unwrap_or(false),
            factory_known: factory_known.unwrap_or(false),
            imported_from_casm: imported_from_casm.unwrap_or(false),
            casm_edited: casm_edited.unwrap_or(false),
            shared_source_voice: shared_source_voice.unwrap_or(false),
            changing_voice: changing_voice.unwrap_or(false),
            tick_zero_converges: tick_zero_converges.unwrap_or(false),
            future_voice_stable: future_voice_stable.unwrap_or(false),
        },
    ))
}
#[tauri::command]
fn resolve_catalog_voice(
    section: u64,
    channel: u8,
    revision: u64,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<Option<voice_catalog::Voice>, String> {
    let address = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        if session.revision != revision {
            return Err("Document changed; refresh the voice lookup".into());
        }
        let tick = session
            .document
            .sections
            .iter()
            .find(|s| s.id == section)
            .ok_or("section not found")?
            .start;
        voice_catalog::address_at(&session.document, tick, channel)
    };
    Ok(voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .and_then(|c| address.and_then(|a| c.find(a).cloned())))
}
#[tauri::command]
fn apply_catalog_voice(
    section: u64,
    channel: u8,
    revision: u64,
    address: Address,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let catalog = voice_catalog::load(&catalog_file(&app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List first")?;
    if catalog.find(address).is_none() {
        return Err("Voice is not in the imported catalog".into());
    }
    let recovery = recovery_path(&app)?;
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before applying a voice".into());
    }
    let session = s.session.as_mut().ok_or("Open a Style first")?;
    session
        .execute(
            revision,
            Command::SetVoice {
                section,
                channel,
                address,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
fn checkpoint(session: &Session, path: &Path) -> View {
    let mut view = session.view();
    let result = if view.dirty {
        project::session_bytes(session).and_then(|b| project::write_recovery(path, &b))
    } else if path.exists() {
        std::fs::remove_file(path).map_err(genos_core::Error::Io)
    } else {
        Ok(())
    };
    if let Err(e) = result {
        view.warnings.push(format!("Autosave failed: {e}"));
    }
    view
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeDrumContext {
    header: genos_core::native_drums::Inspection,
    kit_name: String,
}
#[tauri::command]
fn inspect_style_effects(
    revision: u64,
    state: State<'_, Shared>,
) -> Result<genos_core::style_effects::Header, String> {
    let s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if session.revision != revision {
        return Err("Document changed; reopen Effect types".into());
    }
    genos_core::style_effects::inspect(&session.document).map_err(error)
}
#[tauri::command]
fn preview_style_effects(revision: u64, state: State<'_, Shared>) -> Result<EffectPreview, String> {
    let s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if session.revision != revision {
        return Err("Document changed; select initial effect preview again".into());
    }
    Ok(EffectPreview {
        token: format!("{}:{}", s.epoch, session.revision),
        setup: genos_core::style_effects::preview_setup(&session.document).map_err(error)?,
    })
}
#[tauri::command]
fn apply_style_effects(
    revision: u64,
    changes: Vec<genos_core::style_effects::Change>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    if changes.is_empty() || changes.len() > 2 {
        return Err("Choose one or two system-effect blocks".into());
    }
    let catalog = if changes.iter().any(|c| c.effect.is_some()) {
        Some(
            effect_catalog::load(&effect_catalog_file(&app)?)
                .map_err(error)?
                .ok_or("Import the Genos effect Data List first")?,
        )
    } else {
        None
    };
    for change in &changes {
        if let Some(address) = change.effect
            && !catalog.as_ref().is_some_and(|c| {
                c.effects.iter().any(|e| {
                    e.block == change.block.name() && e.msb == address.msb && e.lsb == address.lsb
                })
            })
        {
            return Err("Effect type is not in the imported catalog".into());
        }
    }
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before editing Style effects".into());
    }
    let session = s.session.as_mut().ok_or("Open a Style first")?;
    session
        .execute(revision, Command::SetStyleEffects { changes })
        .map_err(error)?;
    Ok(checkpoint(session, &recovery_path(&app)?))
}
fn native_drum_kit(app: &tauri::AppHandle, address: Address) -> Result<String, String> {
    let catalog = voice_catalog::load(&catalog_file(app)?)
        .map_err(error)?
        .ok_or("Import the Genos Data List in the voice browser first")?;
    catalog
        .find(address)
        .filter(|v| v.category == voice_catalog::DRUM_KIT_CATEGORY)
        .map(|v| v.name.clone())
        .ok_or("Initial drum kit is not in the imported Genos factory catalog".into())
}
#[tauri::command]
fn inspect_native_drums(
    channel: u8,
    revision: u64,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<NativeDrumContext, String> {
    let s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if session.revision != revision {
        return Err("Document changed; reopen Drum Setup".into());
    }
    let header = genos_core::native_drums::inspect(&session.document, channel).map_err(error)?;
    let kit_name = native_drum_kit(&app, header.address)?;
    Ok(NativeDrumContext { header, kit_name })
}
#[tauri::command]
fn apply_native_drums(
    channel: u8,
    revision: u64,
    address: Address,
    changes: Vec<genos_core::native_drums::Change>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before changing Drum Setup".into());
    }
    native_drum_kit(&app, address)?;
    let session = s.session.as_mut().ok_or("Open a Style first")?;
    session
        .execute(
            revision,
            Command::SetDrumScalars {
                channel,
                address,
                changes,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery_path(&app)?))
}
fn remember(app: &tauri::AppHandle, path: &Path) {
    if let Ok(dir) = app_data(app) {
        let _ = recent::record(&dir, path);
    }
}
#[tauri::command]
async fn open_document(
    path: String,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let path_buf = PathBuf::from(&path);
    let session =
        tauri::async_runtime::spawn_blocking(move || project::open_file(Path::new(&path)))
            .await
            .map_err(error)?
            .map_err(error)?;
    let mut s = state.lock().map_err(error)?;
    // Stop any prior playback; never auto-send MIDI just because a file opened.
    if s.capture.is_recording() {
        return Err("Stop MIDI capture before replacing the document".into());
    }
    s.transport.stop();
    let view = session.view();
    s.session = Some(session);
    s.epoch += 1;
    drop(s);
    remember(&app, &path_buf);
    Ok(view)
}
#[tauri::command]
async fn create_blank_document(
    reference_path: String,
    options: BlankOptions,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let session = tauri::async_runtime::spawn_blocking(move || {
        if std::fs::metadata(&reference_path).map_err(error)?.len()
            > genos_core::smf::MAX_BYTES as u64
        {
            return Err("Reference exceeds the 64 MiB input limit".to_string());
        }
        let bytes = std::fs::read(Path::new(&reference_path)).map_err(error)?;
        blank::create_blank_from_reference(&bytes, options).map_err(error)
    })
    .await
    .map_err(error)??;
    let mut s = state.lock().map_err(error)?;
    s.transport.stop();
    if s.capture.is_recording() {
        return Err("Stop MIDI capture before creating a document".into());
    }
    let view = checkpoint(&session, &recovery_path(&app)?);
    s.session = Some(session);
    s.epoch += 1;
    Ok(view)
}
#[tauri::command]
async fn edit_document(
    revision: u64,
    command: Command,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    if matches!(
        &command,
        Command::SetDrumScalars { .. } | Command::SetStyleEffects { .. }
    ) {
        return Err("Use the catalog-validated Drum Setup or Style effect command".into());
    }
    let recovery = recovery_path(&app)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        let session = s.session.as_mut().ok_or("Open a Style first")?;
        session.execute(revision, command).map_err(error)?;
        Ok(checkpoint(session, &recovery))
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn history(
    redo: bool,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let recovery = recovery_path(&app)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        let session = s.session.as_mut().ok_or("Open a Style first")?;
        if redo { session.redo() } else { session.undo() }.map_err(error)?;
        Ok(checkpoint(session, &recovery))
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn save_document(
    path: String,
    project_file: bool,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let path_buf = PathBuf::from(&path);
    let (bytes, revision, epoch) = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        let bytes = if project_file {
            project::session_bytes(session).map_err(error)?
        } else {
            session.document.style.bytes().map_err(error)?
        };
        (bytes, session.revision, s.epoch)
    };
    tauri::async_runtime::spawn_blocking(move || project::write_new(Path::new(&path), &bytes))
        .await
        .map_err(error)?
        .map_err(error)?;
    remember(&app, &path_buf);
    let mut s = state.lock().map_err(error)?;
    if s.epoch != epoch {
        return Err("File saved, but the active document changed during save".into());
    }
    let session = s.session.as_mut().ok_or("Project changed during save")?;
    if session.revision == revision {
        session.mark_saved().map_err(error)?;
    }
    Ok(checkpoint(session, &recovery_path(&app)?))
}
#[tauri::command]
fn recovery_status(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(recovery_path(&app)?.is_file())
}
#[tauri::command]
async fn restore_recovery(app: tauri::AppHandle, state: State<'_, Shared>) -> Result<View, String> {
    let path = recovery_path(&app)?;
    let mut session = tauri::async_runtime::spawn_blocking(move || project::open_file(&path))
        .await
        .map_err(error)?
        .map_err(error)?;
    session.mark_unsaved();
    let view = session.view();
    let mut s = state.lock().map_err(error)?;
    s.transport.stop();
    s.session = Some(session);
    s.epoch += 1;
    Ok(view)
}
#[tauri::command]
fn discard_recovery(app: tauri::AppHandle) -> Result<(), String> {
    let p = recovery_path(&app)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(error)?;
    }
    Ok(())
}
#[tauri::command]
fn list_recent(app: tauri::AppHandle) -> Result<Vec<RecentEntryView>, String> {
    recent::list(&app_data(&app)?).map_err(error)
}
#[tauri::command]
fn remove_recent(id: String, app: tauri::AppHandle) -> Result<Vec<RecentEntryView>, String> {
    recent::remove(&app_data(&app)?, &id).map_err(error)
}
#[tauri::command]
fn load_prefs(app: tauri::AppHandle) -> Result<Prefs, String> {
    prefs::load(&prefs_file(&app)?).map_err(error)
}
#[tauri::command]
fn save_prefs(prefs: Prefs, app: tauri::AppHandle) -> Result<(), String> {
    prefs::save(&prefs_file(&app)?, &prefs).map_err(error)
}
#[tauri::command]
async fn scan_devices() -> Result<midi::Devices, String> {
    tauri::async_runtime::spawn_blocking(midi::devices)
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
fn play(
    section: u64,
    channels: Vec<u8>,
    index: usize,
    name: String,
    looping: bool,
    effects_token: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    let initial_effects = effects_token.is_some();
    if let Some(token) = effects_token {
        if token != format!("{}:{}", s.epoch, session.revision) {
            return Err("Document changed; select initial effect preview again".into());
        }
        if s.capture.is_recording() {
            return Err("Stop capture before previewing initial effects".into());
        }
    }
    let doc = session.document.clone();
    s.transport
        .play_with_options(
            &doc,
            midi::PlaybackOptions {
                section,
                channels,
                index,
                name,
                looping,
                initial_effects,
            },
        )
        .map_err(error)
}
#[tauri::command]
fn stop(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.transport.stop();
    Ok(())
}
#[tauri::command]
fn panic_release(index: usize, name: String, channels: Option<Vec<u8>>) -> Result<(), String> {
    let ch = channels.unwrap_or_else(|| (1u8..=16).collect());
    midi::panic_release(index, &name, &ch).map_err(error)
}
#[tauri::command]
fn test_note(
    index: usize,
    name: String,
    channel: Option<u8>,
    pitch: Option<u8>,
    velocity: Option<u8>,
    hold_ms: Option<u64>,
) -> Result<(), String> {
    midi::test_note(
        index,
        &name,
        channel.unwrap_or(1),
        pitch.unwrap_or(60),
        velocity.unwrap_or(80),
        hold_ms.unwrap_or(250),
    )
    .map_err(error)
}
#[tauri::command]
fn transport_status(state: State<'_, Shared>) -> Result<midi::Status, String> {
    Ok(state.lock().map_err(error)?.transport.status())
}
#[tauri::command]
fn start_monitor(name: String, state: State<'_, Shared>) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    if s.capture.is_recording() {
        return Err("Stop capture before starting the input monitor".into());
    }
    s.monitor.start(&name).map_err(error)
}
#[tauri::command]
fn stop_monitor(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.monitor.stop();
    Ok(())
}
#[tauri::command]
fn monitor_snapshot(
    state: State<'_, Shared>,
) -> Result<(Option<String>, Vec<midi::MonitorEvent>), String> {
    Ok(state.lock().map_err(error)?.monitor.snapshot())
}
#[tauri::command]
fn clear_monitor(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.monitor.clear();
    Ok(())
}
#[tauri::command]
fn start_capture(name: String, state: State<'_, Shared>) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    if s.monitor.is_active() {
        return Err("Stop the input monitor before starting capture".into());
    }
    s.capture.start(&name).map_err(error)
}
#[tauri::command]
fn stop_capture(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.capture.stop();
    Ok(())
}
#[tauri::command]
fn capture_snapshot(state: State<'_, Shared>) -> Result<CaptureSnapshot, String> {
    Ok(state.lock().map_err(error)?.capture.snapshot_tail(128))
}
#[tauri::command]
fn capture_marker(
    label: String,
    state: State<'_, Shared>,
) -> Result<capture::CaptureMarker, String> {
    state
        .lock()
        .map_err(error)?
        .capture
        .add_marker(&label)
        .map_err(error)
}
#[tauri::command]
fn clear_capture(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.capture.clear().map_err(error)
}
#[tauri::command]
async fn export_capture_midi(path: String, state: State<'_, Shared>) -> Result<(), String> {
    let bytes = {
        let s = state.lock().map_err(error)?;
        s.capture.to_smf_bytes().map_err(error)?
    };
    tauri::async_runtime::spawn_blocking(move || {
        project::write_new(Path::new(&path), &bytes).map_err(error)
    })
    .await
    .map_err(error)??;
    Ok(())
}
#[tauri::command]
async fn export_section_csv(
    path: String,
    section: u64,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let csv = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        export_csv::section_notes_csv(&session.document, section).map_err(error)?
    };
    tauri::async_runtime::spawn_blocking(move || {
        project::write_new(Path::new(&path), csv.as_bytes()).map_err(error)
    })
    .await
    .map_err(error)??;
    Ok(())
}
#[tauri::command]
async fn export_section_json(
    path: String,
    section: u64,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let text = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        export_json::section_snapshot_pretty(&session.document, section).map_err(error)?
    };
    tauri::async_runtime::spawn_blocking(move || {
        project::write_new(Path::new(&path), text.as_bytes()).map_err(error)
    })
    .await
    .map_err(error)??;
    Ok(())
}
#[tauri::command]
fn apply_capture_to_section(
    section: u64,
    revision: u64,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let recovery = recovery_path(&app)?;
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing {
        return Err("Stop playback before applying capture".into());
    }
    let (start, end, ppq, tempo) = {
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        let sec = session
            .document
            .sections
            .iter()
            .find(|x| x.id == section)
            .ok_or("section not found")?;
        let tempo = session.document.timing_at(sec.start).0;
        if session.document.style.tracks.iter().flatten().any(|event| {
            event.tick > sec.start && event.tick < sec.end && event.meta(0x51).is_some()
        }) {
            return Err("Applying capture to a section with tempo changes is not qualified; export the raw MIDI take instead".into());
        }
        (sec.start, sec.end, session.document.style.ppq, tempo)
    };
    let notes = s
        .capture
        .to_note_inputs(start, end, ppq, tempo, 0)
        .map_err(error)?;
    let session = s.session.as_mut().ok_or("Open a Style first")?;
    session
        .execute(revision, Command::AddNotes { section, notes })
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
#[tauri::command]
async fn export_phrase_midi(
    path: String,
    section: u64,
    channel: u8,
    revision: u64,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let bytes = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        if session.revision != revision {
            return Err("The document changed; reopen MIDI exchange".into());
        }
        exchange::export(&session.document, section, channel).map_err(error)?
    };
    tauri::async_runtime::spawn_blocking(move || {
        project::write_new(Path::new(&path), &bytes).map_err(error)
    })
    .await
    .map_err(error)??;
    Ok(())
}
#[tauri::command]
async fn preview_phrase_midi(
    path: String,
    section: u64,
    channel: u8,
    revision: u64,
    state: State<'_, Shared>,
) -> Result<ExchangePreview, String> {
    let epoch = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        if session.revision != revision {
            return Err("The document changed; reopen MIDI exchange".into());
        }
        s.epoch
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        // Bounded read as well as metadata check, including files that grow during reading.
        use std::io::Read;
        let file = std::fs::File::open(&path).map_err(error)?;
        if file.metadata().map_err(error)?.len() > exchange::MAX_MIDI_BYTES as u64 {
            return Err("Exchange MIDI exceeds 8 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(exchange::MAX_MIDI_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() > exchange::MAX_MIDI_BYTES {
            return Err("Exchange MIDI exceeds 8 MiB".into());
        }
        Ok(bytes)
    })
    .await
    .map_err(error)??;
    let mut s = state.lock().map_err(error)?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if s.epoch != epoch || session.revision != revision {
        return Err("The document changed; preview the MIDI again".into());
    }
    let prepared = exchange::prepare(&session.document, section, channel, &bytes).map_err(error)?;
    s.exchange_nonce += 1;
    let token = format!("{epoch}-{revision}-{}", s.exchange_nonce);
    s.pending_exchange = Some(PendingExchange {
        token: token.clone(),
        epoch,
        revision,
        section,
        channel,
        notes: prepared.notes,
    });
    Ok(ExchangePreview {
        token,
        preview: prepared.preview,
    })
}
#[tauri::command]
fn apply_phrase_midi(
    token: String,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let recovery = recovery_path(&app)?;
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before importing notes".into());
    }
    let p = s
        .pending_exchange
        .as_ref()
        .filter(|p| p.token == token)
        .ok_or("MIDI preview expired; choose the file again")?;
    let session = s.session.as_ref().ok_or("Open a Style first")?;
    if p.epoch != s.epoch || p.revision != session.revision {
        return Err("The document changed; preview the MIDI again".into());
    }
    let p = s.pending_exchange.take().unwrap();
    let session = s.session.as_mut().unwrap();
    session
        .execute(
            p.revision,
            Command::ReplacePhrase {
                section: p.section,
                channel: p.channel,
                notes: p.notes,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            open_document,
            create_blank_document,
            edit_document,
            history,
            save_document,
            recovery_status,
            restore_recovery,
            discard_recovery,
            list_recent,
            remove_recent,
            load_prefs,
            save_prefs,
            scan_devices,
            play,
            stop,
            panic_release,
            test_note,
            transport_status,
            start_monitor,
            stop_monitor,
            monitor_snapshot,
            clear_monitor,
            start_capture,
            stop_capture,
            capture_snapshot,
            capture_marker,
            clear_capture,
            export_capture_midi,
            export_section_csv,
            export_section_json,
            apply_capture_to_section,
            export_phrase_midi,
            preview_phrase_midi,
            apply_phrase_midi,
            voice_catalog_status,
            import_voice_catalog,
            search_voice_catalog,
            search_drum_kits,
            effect_catalog_status,
            import_effect_catalog,
            search_effect_catalog,
            drum_channel_status,
            resolve_catalog_voice,
            apply_catalog_voice,
            preview_drum_note,
            inspect_native_drums,
            apply_native_drums,
            inspect_style_effects,
            preview_style_effects,
            preview_live_style,
            play_live_style,
            apply_style_effects,
            apply_genos_system_effect,
            chord_preview_vocabulary,
            inspect_chord_preview,
            remap_chord_preview_notes,
            recognize_chord_preview,
            pitch_shift_bend_message,
            pitch_shift_wire_pitch,
            pitch_shift_retarget,
            validate_expansion_guitar_profiles,
            expansion_guitar_candidate_phase1,
            analyze_expansion_pack,
            extract_expansion_resource
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let state = window.state::<Shared>();
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.transport.stop();
                s.monitor.stop();
                s.capture.stop();
            }
        })
        .run(tauri::generate_context!())
        .expect("GenosStyleStudio could not start");
}
