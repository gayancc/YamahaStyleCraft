#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod note_clipboard;
use genos_core::{
    blank::{self, BlankOptions},
    capture::{self, CaptureSnapshot, MidiCapture},
    chord_preview,
    document::View,
    edit::{Command, Session},
    effect_catalog, exchange, expansion_guitar, export_csv, export_json, genos_system_effect,
    library,
    midi::{self, InputMonitor, Transport},
    pitch_shift_preview,
    prefs::{self, Prefs},
    project,
    recent::{self, RecentEntryView},
    voice_catalog::{self, Address},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};

/// Extra project window document (SFM-008). The primary `main` window keeps
/// using the dedicated session fields on [`Studio`] for stability.
struct ProjectDoc {
    session: Session,
    epoch: u64,
    exchange_nonce: u64,
    pending_exchange: Option<PendingExchange>,
    #[allow(dead_code)]
    pending_live: Option<PendingLive>,
}
struct Studio {
    clipboard: Option<note_clipboard::NoteClipboard>,
    session: Option<Session>,
    transport: Transport,
    monitor: InputMonitor,
    capture: MidiCapture,
    epoch: u64,
    exchange_nonce: u64,
    pending_exchange: Option<PendingExchange>,
    pending_live: Option<PendingLive>,
    /// Labels `project-2` … for additional Genos windows.
    projects: HashMap<String, ProjectDoc>,
    /// SFM-VIEW-TRANSPORT-MIDI-OWNER: only one project window owns MIDI hardware.
    midi_owner: String,
}
impl Default for Studio {
    fn default() -> Self {
        Self {
            clipboard: None,
            session: None,
            transport: Transport::default(),
            monitor: InputMonitor::default(),
            capture: MidiCapture::default(),
            epoch: 0,
            exchange_nonce: 0,
            pending_exchange: None,
            pending_live: None,
            projects: HashMap::new(),
            midi_owner: "main".into(),
        }
    }
}
#[tauri::command]
fn write_note_clipboard(
    payload: note_clipboard::NoteClipboard,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    payload.validate()?;
    let mut s = state.lock().map_err(error)?;
    s.clipboard = Some(payload.clone());
    // Keep broadcasts in the same order as writes from competing windows.
    let _ = app.emit("studio://clipboard-sync", &payload);
    Ok(())
}
#[tauri::command]
fn read_note_clipboard(
    state: State<'_, Shared>,
) -> Result<Option<note_clipboard::NoteClipboard>, String> {
    Ok(state.lock().map_err(error)?.clipboard.clone())
}
/// Reject MIDI hardware ops when another project window holds the lease.
fn require_midi_owner(s: &Studio, doc_label: &str) -> Result<(), String> {
    if s.midi_owner == doc_label {
        Ok(())
    } else {
        Err("MIDI belongs to another project window. Choose Use MIDI here first.".into())
    }
}
/// Document generation counter — main uses `Studio.epoch`; project-N uses `ProjectDoc.epoch`.
fn doc_epoch(s: &Studio, label: &str) -> u64 {
    if label == "main" {
        s.epoch
    } else {
        s.projects.get(label).map(|d| d.epoch).unwrap_or(0)
    }
}
/// Next epoch when replacing a secondary project document.
fn next_project_epoch(s: &Studio, label: &str) -> u64 {
    s.projects.get(label).map(|d| d.epoch + 1).unwrap_or(1)
}
fn resolve_doc_label(doc_label: Option<&str>) -> String {
    let label = doc_label.unwrap_or("main");
    if label.is_empty() || label == "piano-roll" || label == "mixer" {
        "main".into()
    } else {
        label.into()
    }
}
fn session_mut<'a>(s: &'a mut Studio, label: &str) -> Result<&'a mut Session, String> {
    if label == "main" {
        s.session
            .as_mut()
            .ok_or_else(|| "Open a Style first".into())
    } else {
        s.projects
            .get_mut(label)
            .map(|d| &mut d.session)
            .ok_or_else(|| "Open a Style first".into())
    }
}
fn session_ref<'a>(s: &'a Studio, label: &str) -> Result<&'a Session, String> {
    if label == "main" {
        s.session
            .as_ref()
            .ok_or_else(|| "Open a Style first".into())
    } else {
        s.projects
            .get(label)
            .map(|d| &d.session)
            .ok_or_else(|| "Open a Style first".into())
    }
}
struct PendingLive {
    token: String,
    epoch: u64,
    revision: u64,
    compiled: genos_core::live_preview::Compiled,
    doc_label: String,
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
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<LivePreview, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let mut s = state.lock().map_err(error)?;
    s.pending_live = None;
    let session = session_ref(&s, &label)?;
    if session.revision != revision {
        return Err("Document changed; check the preview again".into());
    }
    let compiled = genos_core::live_preview::compile(&session.document, &config).map_err(error)?;
    s.exchange_nonce += 1;
    let epoch = doc_epoch(&s, &label);
    let token = format!("live:{epoch}:{revision}:{}", s.exchange_nonce);
    let report = compiled.report.clone();
    s.pending_live = Some(PendingLive {
        token: token.clone(),
        epoch,
        revision,
        compiled,
        doc_label: label,
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
    let label = pending.doc_label.clone();
    require_midi_owner(&s, &label)?;
    let epoch = doc_epoch(&s, &label);
    let session = session_ref(&s, &label)?;
    if pending.token != token || pending.epoch != epoch || pending.revision != session.revision {
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
    /// Non-empty for whole-section multi-track apply (SFM-121).
    phrases: Vec<exchange::ChannelPhrase>,
    /// Project window label (SFM-008); `"main"` for the primary session.
    doc_label: String,
}
fn queue_exchange(s: &mut Studio, mut pending: PendingExchange) -> Result<String, String> {
    let (nonce, slot) = if pending.doc_label == "main" {
        (&mut s.exchange_nonce, &mut s.pending_exchange)
    } else {
        let doc = s
            .projects
            .get_mut(&pending.doc_label)
            .ok_or("Open a Style first")?;
        (&mut doc.exchange_nonce, &mut doc.pending_exchange)
    };
    *nonce = nonce
        .checked_add(1)
        .ok_or("MIDI preview counter overflow")?;
    pending.token = format!(
        "{}:{}:{}:{}",
        pending.doc_label, pending.epoch, pending.revision, nonce
    );
    let token = pending.token.clone();
    *slot = Some(pending);
    Ok(token)
}
fn take_exchange(s: &mut Studio, token: &str) -> Result<PendingExchange, String> {
    let pending = s
        .pending_exchange
        .iter()
        .chain(
            s.projects
                .values()
                .filter_map(|d| d.pending_exchange.as_ref()),
        )
        .find(|p| p.token == token)
        .ok_or("MIDI preview expired; choose the file again")?;
    let label = pending.doc_label.clone();
    if pending.epoch != doc_epoch(s, &label) || pending.revision != session_ref(s, &label)?.revision
    {
        return Err("The document changed; preview the MIDI again".into());
    }
    if label == "main" {
        s.pending_exchange
            .take()
            .ok_or_else(|| "MIDI preview expired".into())
    } else {
        s.projects
            .get_mut(&label)
            .and_then(|d| d.pending_exchange.take())
            .ok_or_else(|| "MIDI preview expired".into())
    }
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
fn recovery_path_for(app: &tauri::AppHandle, doc_label: &str) -> Result<PathBuf, String> {
    if doc_label == "main" {
        recovery_path(app)
    } else {
        let safe: String = doc_label
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Ok(app_data(app)?.join(format!("recovery-{safe}.gss")))
    }
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
    Ok(Some(
        voice_catalog::available(&catalog_file(&app)?).summary(),
    ))
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
        Ok(voice_catalog::available(&destination).summary())
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
    kind: Option<String>,
    category: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    app: tauri::AppHandle,
) -> Result<voice_catalog::Search, String> {
    voice_catalog::available(&catalog_file(&app)?)
        .browse(
            &query,
            kind.as_deref().unwrap_or("all"),
            category.as_deref().unwrap_or(""),
            offset.unwrap_or(0),
            limit.unwrap_or(100),
        )
        .map_err(error)
}
#[tauri::command]
fn search_drum_kits(query: String, app: tauri::AppHandle) -> Result<voice_catalog::Search, String> {
    voice_catalog::available(&catalog_file(&app)?)
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
    let catalog = voice_catalog::available(&catalog_file(&app)?);
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
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
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
fn extract_all_expansion_resources(
    request: genos_core::expansion::ExtractAllRequest,
) -> Result<genos_core::expansion::ExtractAllResult, String> {
    genos_core::expansion::recovery::extract_all_resources(&request).map_err(error)
}

#[tauri::command]
fn rewrite_expansion_ppf(source_path: String, dest_path: String) -> Result<String, String> {
    let src = Path::new(&source_path);
    let data = genos_core::expansion::ppf::read_ppf_file(src).map_err(error)?;
    let pkg = genos_core::expansion::parse_ppf(&data).map_err(error)?;
    let out = genos_core::expansion::rewrite_ppf(&data, &pkg).map_err(error)?;
    let dest = Path::new(&dest_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(error)?;
    }
    project::write_new(dest, &out).map_err(error)?;
    Ok(dest.display().to_string())
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
    let catalog = voice_catalog::available(&catalog_file(&app)?);
    Ok(address.and_then(|a| catalog.find(a).cloned()))
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
    let catalog = voice_catalog::available(&catalog_file(&app)?);
    if catalog.find(address).is_none() {
        return Err("Voice is not in the Genos factory catalog".into());
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
    let catalog = voice_catalog::available(&catalog_file(app)?);
    catalog
        .find(address)
        .filter(|v| v.category == voice_catalog::DRUM_KIT_CATEGORY)
        .map(|v| v.name.clone())
        .ok_or("Initial drum kit is not in the Genos factory catalog".into())
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
#[tauri::command]
fn reset_native_drums(
    channel: u8,
    revision: u64,
    address: Address,
    notes: Vec<u8>,
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
            Command::ResetDrumScalars {
                channel,
                address,
                notes,
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
/// Current session View for detached / project windows (SFM-068 / SFM-008).
#[tauri::command]
fn session_view(doc_label: Option<String>, state: State<'_, Shared>) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let s = state.lock().map_err(error)?;
    if label == "main" {
        Ok(s.session.as_ref().ok_or("Open a Style first")?.view())
    } else {
        Ok(s.projects
            .get(&label)
            .ok_or("Open a Style first")?
            .session
            .view())
    }
}
#[tauri::command]
async fn open_document(
    path: String,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
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
    if label == "main" {
        s.session = Some(session);
        s.epoch += 1;
        s.pending_exchange = None;
        s.pending_live = None;
    } else {
        let epoch = next_project_epoch(&s, &label);
        s.projects.insert(
            label,
            ProjectDoc {
                session,
                epoch,
                exchange_nonce: 0,
                pending_exchange: None,
                pending_live: None,
            },
        );
    }
    drop(s);
    remember(&app, &path_buf);
    Ok(view)
}
#[tauri::command]
async fn create_blank_document(
    reference_path: String,
    options: BlankOptions,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
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
    let recovery = recovery_path_for(&app, &label)?;
    let view = checkpoint(&session, &recovery);
    if label == "main" {
        s.session = Some(session);
        s.epoch += 1;
        s.pending_exchange = None;
        s.pending_live = None;
    } else {
        let epoch = next_project_epoch(&s, &label);
        s.projects.insert(
            label,
            ProjectDoc {
                session,
                epoch,
                exchange_nonce: 0,
                pending_exchange: None,
                pending_live: None,
            },
        );
    }
    Ok(view)
}
#[tauri::command]
async fn edit_document(
    revision: u64,
    command: Command,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    if matches!(
        &command,
        Command::SetDrumScalars { .. }
            | Command::ResetDrumScalars { .. }
            | Command::SetStyleEffects { .. }
    ) {
        return Err("Use the catalog-validated Drum Setup or Style effect command".into());
    }
    let label = resolve_doc_label(doc_label.as_deref());
    let recovery = recovery_path_for(&app, &label)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        if matches!(&command, Command::SetVoice { .. }) && s.capture.is_recording() {
            return Err("Stop capture before applying a voice".into());
        }
        if label == "main" {
            let session = s.session.as_mut().ok_or("Open a Style first")?;
            session.execute(revision, command).map_err(error)?;
            Ok(checkpoint(session, &recovery))
        } else {
            let doc = s.projects.get_mut(&label).ok_or("Open a Style first")?;
            doc.session.execute(revision, command).map_err(error)?;
            Ok(checkpoint(&doc.session, &recovery))
        }
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn history(
    redo: bool,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let recovery = recovery_path_for(&app, &label)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        if label == "main" {
            let session = s.session.as_mut().ok_or("Open a Style first")?;
            if redo { session.redo() } else { session.undo() }.map_err(error)?;
            Ok(checkpoint(session, &recovery))
        } else {
            let doc = s.projects.get_mut(&label).ok_or("Open a Style first")?;
            if redo {
                doc.session.redo()
            } else {
                doc.session.undo()
            }
            .map_err(error)?;
            Ok(checkpoint(&doc.session, &recovery))
        }
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn save_document(
    path: String,
    project_file: bool,
    doc_label: Option<String>,
    ui_state: Option<project::ProjectUiState>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let path_buf = PathBuf::from(&path);
    let (bytes, revision, epoch) = {
        let mut s = state.lock().map_err(error)?;
        if label == "main" {
            let session = s.session.as_mut().ok_or("Open a Style first")?;
            if project_file && let Some(ui) = ui_state.clone() {
                session.set_ui_state(ui);
            }
            let bytes = if project_file {
                project::session_bytes(session).map_err(error)?
            } else {
                session.document.style.bytes().map_err(error)?
            };
            (bytes, session.revision, s.epoch)
        } else {
            let doc = s.projects.get_mut(&label).ok_or("Open a Style first")?;
            if project_file && let Some(ui) = ui_state {
                doc.session.set_ui_state(ui);
            }
            let bytes = if project_file {
                project::session_bytes(&doc.session).map_err(error)?
            } else {
                doc.session.document.style.bytes().map_err(error)?
            };
            (bytes, doc.session.revision, doc.epoch)
        }
    };
    tauri::async_runtime::spawn_blocking(move || project::write_new(Path::new(&path), &bytes))
        .await
        .map_err(error)?
        .map_err(error)?;
    remember(&app, &path_buf);
    let mut s = state.lock().map_err(error)?;
    let recovery = recovery_path_for(&app, &label)?;
    if label == "main" {
        if s.epoch != epoch {
            return Err("File saved, but the active document changed during save".into());
        }
        let session = s.session.as_mut().ok_or("Project changed during save")?;
        if session.revision == revision {
            session.mark_saved().map_err(error)?;
        }
        Ok(checkpoint(session, &recovery))
    } else {
        let doc = s
            .projects
            .get_mut(&label)
            .ok_or("Project changed during save")?;
        if doc.epoch != epoch {
            return Err("File saved, but the active document changed during save".into());
        }
        if doc.session.revision == revision {
            doc.session.mark_saved().map_err(error)?;
        }
        Ok(checkpoint(&doc.session, &recovery))
    }
}
#[tauri::command]
fn recovery_status(doc_label: Option<String>, app: tauri::AppHandle) -> Result<bool, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    Ok(recovery_path_for(&app, &label)?.is_file())
}
#[tauri::command]
async fn restore_recovery(
    doc_label: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let path = recovery_path_for(&app, &label)?;
    let mut session = tauri::async_runtime::spawn_blocking(move || project::open_file(&path))
        .await
        .map_err(error)?
        .map_err(error)?;
    session.mark_unsaved();
    let view = session.view();
    let mut s = state.lock().map_err(error)?;
    s.transport.stop();
    if label == "main" {
        s.session = Some(session);
        s.epoch += 1;
    } else {
        let epoch = next_project_epoch(&s, &label);
        s.projects.insert(
            label,
            ProjectDoc {
                session,
                epoch,
                exchange_nonce: 0,
                pending_exchange: None,
                pending_live: None,
            },
        );
    }
    Ok(view)
}
#[tauri::command]
fn discard_recovery(doc_label: Option<String>, app: tauri::AppHandle) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let p = recovery_path_for(&app, &label)?;
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
#[allow(clippy::too_many_arguments)] // Tauri IPC command surface
fn play(
    section: u64,
    channels: Vec<u8>,
    index: usize,
    name: String,
    looping: bool,
    effects_token: Option<String>,
    from_tick: Option<u64>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let mut s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
    let session = session_ref(&s, &label)?;
    let initial_effects = effects_token.is_some();
    if let Some(token) = effects_token {
        if token != format!("{}:{}", doc_epoch(&s, &label), session.revision) {
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
                from_tick,
            },
        )
        .map_err(error)
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MidiOwnership {
    owner: String,
    active: bool,
    windows: usize,
}
#[tauri::command]
fn midi_ownership(
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<MidiOwnership, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let s = state.lock().map_err(error)?;
    Ok(MidiOwnership {
        owner: s.midi_owner.clone(),
        active: s.midi_owner == label,
        windows: 1 + s.projects.len(),
    })
}
/// Claim MIDI hardware for this project window (SFM Use MIDI here).
#[tauri::command]
fn claim_midi(
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<MidiOwnership, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let mut s = state.lock().map_err(error)?;
    if s.midi_owner != label {
        // SFM: stop play/capture/monitor before switching the lease.
        if s.transport.status().playing {
            s.transport.stop();
        }
        if s.capture.is_recording() {
            return Err("Stop recording in the other project window before switching MIDI.".into());
        }
        if s.monitor.is_active() {
            s.monitor.stop();
        }
        s.midi_owner = label.clone();
    }
    Ok(MidiOwnership {
        owner: s.midi_owner.clone(),
        active: true,
        windows: 1 + s.projects.len(),
    })
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
#[allow(clippy::too_many_arguments)] // Tauri IPC command surface
fn test_note(
    index: usize,
    name: String,
    channel: Option<u8>,
    pitch: Option<u8>,
    velocity: Option<u8>,
    hold_ms: Option<u64>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
    drop(s);
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
fn start_monitor(
    name: String,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let mut s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
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
/// Held pitches on the active input monitor for MIDI LH chord recognition (SFM-045).
#[tauri::command]
fn monitor_held_notes(
    // 1..=16 filter; None/0 = all channels (SFM chordPreviewChannel).
    channel: Option<u8>,
    // Inclusive LH upper pitch; None = no split (SFM chordPreviewSplit default 59).
    max_pitch: Option<u8>,
    state: State<'_, Shared>,
) -> Result<Vec<u8>, String> {
    let ch = channel.filter(|c| (1..=16).contains(c));
    Ok(state
        .lock()
        .map_err(error)?
        .monitor
        .held_notes(ch, max_pitch))
}
#[tauri::command]
fn clear_monitor(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.monitor.clear();
    Ok(())
}
#[tauri::command]
fn start_capture(
    name: String,
    thru_index: Option<usize>,
    thru_name: Option<String>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let mut s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
    if s.monitor.is_active() {
        return Err("Stop the input monitor before starting capture".into());
    }
    // SFM: Record while Play is running stops ordinary playback first, then
    // the UI starts overdub playback of existing parts under Mute/Solo.
    if s.transport.status().playing {
        s.transport.stop();
    }
    let thru = match (thru_index, thru_name.as_deref()) {
        (Some(i), Some(n)) => Some((i, n)),
        (None, None) => None,
        _ => {
            return Err("MIDI thru requires both output index and name".into());
        }
    };
    s.capture.start(&name, thru).map_err(error)
}
#[tauri::command]
fn stop_capture(state: State<'_, Shared>) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    s.capture.stop();
    // End overdub accompaniment with the take (SFM parts recording).
    if s.transport.status().playing {
        s.transport.stop();
    }
    Ok(())
}
/// Live mute/solo during playback / overdub (SFM M/S). Releases newly muted parts.
#[tauri::command]
fn set_playback_channels(
    channels: Vec<u8>,
    index: usize,
    name: String,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let muted = {
        let s = state.lock().map_err(error)?;
        let prev = s.transport.audible_channels();
        s.transport
            .set_audible_channels(channels.clone())
            .map_err(error)?;
        if !s.transport.status().playing {
            return Ok(());
        }
        prev.into_iter()
            .filter(|ch| !channels.contains(ch))
            .collect::<Vec<_>>()
    };
    if muted.is_empty() {
        return Ok(());
    }
    // Drop Shared lock before WinMM I/O so other commands stay responsive.
    midi::panic_release(index, &name, &muted).map_err(error)
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
fn capture_inject_cc(
    channel: u8,
    controller: u8,
    value: u8,
    state: State<'_, Shared>,
) -> Result<genos_core::midi::MonitorEvent, String> {
    state
        .lock()
        .map_err(error)?
        .capture
        .inject_cc(channel, controller, value)
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
#[allow(clippy::too_many_arguments)] // Tauri IPC command surface
fn apply_capture_to_section(
    section: u64,
    revision: u64,
    quantize_step: Option<u64>,
    offset_ms: Option<i32>,
    destination_channel: Option<u8>,
    // SFM Record data: include paired notes / CC+PC+bend (default both true).
    include_notes: Option<bool>,
    include_controllers: Option<bool>,
    // 0/None = OMNI (all channels); 1..=16 = keep only that input channel before arm remap.
    input_channel: Option<u8>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let recovery = recovery_path_for(&app, &label)?;
    let mut s = state.lock().map_err(error)?;
    require_midi_owner(&s, &label)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before applying the take".into());
    }
    let (start, end, ppq, tempo) = {
        let session = session_ref(&s, &label)?;
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
    let step = quantize_step.unwrap_or(0);
    let offset = offset_ms.unwrap_or(0);
    let want_notes = include_notes.unwrap_or(true);
    let want_controllers = include_controllers.unwrap_or(true);
    if !want_notes && !want_controllers {
        return Err("Record data must include notes and/or controllers".into());
    }
    let mut notes = if want_notes {
        match s
            .capture
            .to_note_inputs_quantized(start, end, ppq, tempo, 0, step, offset)
        {
            Ok(n) => n,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("no complete note on/off pairs") {
                    Vec::new()
                } else {
                    return Err(error(e));
                }
            }
        }
    } else {
        Vec::new()
    };
    let mut channel_events = if want_controllers {
        s.capture
            .to_channel_events_quantized(start, end, ppq, tempo, 0, step, offset)
            .map_err(error)?
    } else {
        Vec::new()
    };
    genos_core::capture::retain_input_channel(&mut notes, &mut channel_events, input_channel);
    if let Some(ch) = destination_channel {
        if !(1..=16).contains(&ch) {
            return Err("armed destination channel must be 1..16".into());
        }
        for n in &mut notes {
            n.channel = ch;
        }
        for ev in &mut channel_events {
            if ev.data.is_empty() || ev.data[0] >= 0xf0 {
                continue;
            }
            ev.data[0] = (ev.data[0] & 0xf0) | (ch - 1);
        }
    }
    if notes.is_empty() && channel_events.is_empty() {
        return Err(if want_notes && !want_controllers {
            "capture has no complete note on/off pairs to apply".into()
        } else if !want_notes && want_controllers {
            "capture has no channel controllers to apply".into()
        } else {
            "capture has no complete notes or channel controllers to apply".into()
        });
    }
    let session = session_mut(&mut s, &label)?;
    session
        .execute(
            revision,
            Command::AddNotes {
                section,
                notes,
                channel_events,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
#[tauri::command]
async fn export_phrase_midi(
    path: String,
    section: u64,
    channel: u8,
    revision: u64,
    whole_section: Option<bool>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let bytes = {
        let s = state.lock().map_err(error)?;
        let session = session_ref(&s, &label)?;
        if session.revision != revision {
            return Err("The document changed; reopen MIDI exchange".into());
        }
        if whole_section.unwrap_or(false) {
            exchange::export_section(&session.document, section).map_err(error)?
        } else {
            exchange::export(&session.document, section, channel).map_err(error)?
        }
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
    whole_section: Option<bool>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
) -> Result<ExchangePreview, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let epoch = {
        let s = state.lock().map_err(error)?;
        let session = session_ref(&s, &label)?;
        if session.revision != revision {
            return Err("The document changed; reopen MIDI exchange".into());
        }
        doc_epoch(&s, &label)
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
    let session = session_ref(&s, &label)?;
    if doc_epoch(&s, &label) != epoch || session.revision != revision {
        return Err("The document changed; preview the MIDI again".into());
    }
    let prepared = if whole_section.unwrap_or(false) {
        exchange::prepare_section(&session.document, section, &bytes).map_err(error)?
    } else {
        exchange::prepare(&session.document, section, channel, &bytes).map_err(error)?
    };
    let token = queue_exchange(
        &mut s,
        PendingExchange {
            token: String::new(),
            epoch,
            revision,
            section,
            channel,
            notes: prepared.notes,
            phrases: prepared.phrases,
            doc_label: label,
        },
    )?;
    Ok(ExchangePreview {
        token,
        preview: prepared.preview,
    })
}
#[tauri::command]
async fn inspect_library_style(path: String) -> Result<library::LibraryStyle, String> {
    tauri::async_runtime::spawn_blocking(move || {
        library::inspect_file(Path::new(&path)).map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn scan_library_folder(path: String) -> Result<Vec<library::LibraryStyle>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        library::scan_folder(Path::new(&path)).map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn preview_library_phrase(
    path: String,
    library_section: u64,
    library_channel: Option<u8>,
    index: usize,
    name: String,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let material = tauri::async_runtime::spawn_blocking(move || {
        library::preview_material(Path::new(&path), library_section, library_channel).map_err(error)
    })
    .await
    .map_err(error)??;
    let (tempo_us, ppq, len, phrases) = material;
    let refs: Vec<(u8, &[exchange::PhraseNote])> = phrases
        .iter()
        .map(|p| (p.channel, p.notes.as_slice()))
        .collect();
    let plan = midi::plan_from_phrases(&refs, ppq, tempo_us, len).map_err(error)?;
    let channels: Vec<u8> = phrases.iter().map(|p| p.channel).collect();
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before library preview".into());
    }
    s.transport
        .play_plan(plan, channels, index, name, false)
        .map_err(error)
}
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri IPC command surface
async fn import_library_section(
    path: String,
    library_section: u64,
    destination_section: u64,
    fit: String,
    revision: u64,
    drums_only: Option<bool>,
    include_voice: Option<bool>,
    include_controllers: Option<bool>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let recovery = recovery_path_for(&app, &label)?;
    let mode = library::LibraryFit::parse(&fit).map_err(error)?;
    let drums = drums_only.unwrap_or(false);
    let want_voice = include_voice.unwrap_or(false) && !drums;
    let want_cc = include_controllers.unwrap_or(false) && !drums;
    let path_for_voices = path.clone();
    let path_for_cc = path.clone();
    let (src_len, phrases) = tauri::async_runtime::spawn_blocking(move || {
        if drums {
            library::extract_drum_phrases(Path::new(&path), library_section).map_err(error)
        } else {
            library::extract_section_phrases(Path::new(&path), library_section).map_err(error)
        }
    })
    .await
    .map_err(error)??;
    let channels: Vec<u8> = phrases.iter().map(|p| p.channel).collect();
    let voices = if want_voice {
        let ch = channels.clone();
        tauri::async_runtime::spawn_blocking(move || {
            library::extract_section_voices(Path::new(&path_for_voices), library_section, &ch)
                .map_err(error)
        })
        .await
        .map_err(error)??
    } else {
        Vec::new()
    };
    let src_controllers = if want_cc {
        tauri::async_runtime::spawn_blocking(move || {
            library::extract_section_controllers(
                Path::new(&path_for_cc),
                library_section,
                &channels,
            )
            .map_err(error)
        })
        .await
        .map_err(error)??
    } else {
        Vec::new()
    };
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before importing a library section".into());
    }
    let session = session_mut(&mut s, &label)?;
    let (dest_start, dest_len) = session
        .document
        .sections
        .iter()
        .find(|x| x.id == destination_section)
        .map(|d| (d.start, d.end.saturating_sub(d.start)))
        .ok_or("destination section not found")?;
    let mut mapped = Vec::with_capacity(phrases.len());
    for p in phrases {
        let notes =
            library::map_to_destination(&p.notes, src_len, dest_len, mode).map_err(error)?;
        mapped.push(exchange::ChannelPhrase {
            channel: p.channel,
            notes,
        });
    }
    let controllers =
        library::map_section_controllers(&src_controllers, src_len, dest_len, dest_start, mode)
            .map_err(error)?;
    session
        .execute(
            revision,
            Command::ReplaceSectionPhrases {
                section: destination_section,
                phrases: mapped,
                voices,
                controllers,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri IPC command surface
async fn import_library_phrase(
    path: String,
    library_section: u64,
    library_channel: u8,
    destination_section: u64,
    destination_channel: u8,
    fit: String,
    revision: u64,
    include_voice: Option<bool>,
    include_controllers: Option<bool>,
    doc_label: Option<String>,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let label = resolve_doc_label(doc_label.as_deref());
    let recovery = recovery_path_for(&app, &label)?;
    let mode = library::LibraryFit::parse(&fit).map_err(error)?;
    let want_voice = include_voice.unwrap_or(false);
    let want_cc = include_controllers.unwrap_or(false);
    let path_for_voice = path.clone();
    let path_for_cc = path.clone();
    let (src_len, phrase) = tauri::async_runtime::spawn_blocking(move || {
        library::extract_phrase(Path::new(&path), library_section, library_channel).map_err(error)
    })
    .await
    .map_err(error)??;
    let voice = if want_voice {
        tauri::async_runtime::spawn_blocking(move || {
            library::extract_voice(Path::new(&path_for_voice), library_section, library_channel)
                .map_err(error)
        })
        .await
        .map_err(error)??
    } else {
        None
    };
    let src_controllers = if want_cc {
        tauri::async_runtime::spawn_blocking(move || {
            library::extract_controllers(Path::new(&path_for_cc), library_section, library_channel)
                .map_err(error)
        })
        .await
        .map_err(error)??
    } else {
        Vec::new()
    };
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before importing a library phrase".into());
    }
    let session = session_mut(&mut s, &label)?;
    let (dest_start, dest_len) = session
        .document
        .sections
        .iter()
        .find(|x| x.id == destination_section)
        .map(|d| (d.start, d.end.saturating_sub(d.start)))
        .ok_or("destination section not found")?;
    let notes = library::map_to_destination(&phrase, src_len, dest_len, mode).map_err(error)?;
    let controllers =
        library::map_controllers(&src_controllers, src_len, dest_len, dest_start, mode)
            .map_err(error)?;
    session
        .execute(
            revision,
            Command::ReplacePhrase {
                section: destination_section,
                channel: destination_channel,
                notes,
                voice,
                controllers,
            },
        )
        .map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
#[tauri::command]
fn apply_phrase_midi(
    token: String,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let mut s = state.lock().map_err(error)?;
    if s.transport.status().playing || s.capture.is_recording() {
        return Err("Stop playback and capture before importing notes".into());
    }
    let p = take_exchange(&mut s, &token)?;
    let label = p.doc_label.clone();
    let recovery = recovery_path_for(&app, &label)?;
    let session = session_mut(&mut s, &label)?;
    let command = if p.phrases.is_empty() {
        Command::ReplacePhrase {
            section: p.section,
            channel: p.channel,
            notes: p.notes,
            voice: None,
            controllers: vec![],
        }
    } else {
        Command::ReplaceSectionPhrases {
            section: p.section,
            phrases: p.phrases,
            voices: vec![],
            controllers: vec![],
        }
    };
    session.execute(p.revision, command).map_err(error)?;
    Ok(checkpoint(session, &recovery))
}
fn configure_webview_context_menus(webview: &tauri::Webview) {
    #[cfg(windows)]
    {
        if let Err(err) = webview.with_webview(move |native| {
            // DOM cancellation alone cannot control WebView2's native menu.
            // This applies to every main, project and detached WebView.
            let result = (|| -> Result<bool, String> {
                unsafe {
                    let settings = native
                        .controller()
                        .CoreWebView2()
                        .map_err(error)?
                        .Settings()
                        .map_err(error)?;
                    settings
                        .SetAreDefaultContextMenusEnabled(false)
                        .map_err(error)?;
                    let mut enabled = Default::default();
                    settings
                        .AreDefaultContextMenusEnabled(&mut enabled)
                        .map_err(error)?;
                    Ok(enabled.as_bool())
                }
            })();
            match result {
                Ok(enabled) => {
                    if enabled {
                        eprintln!("WebView2 default context menus remained enabled");
                    }
                }
                Err(err) => eprintln!("Could not disable WebView2 default context menus: {err}"),
            }
        }) {
            eprintln!("Could not configure WebView2 context menus: {err}");
        }
    }
}

#[tauri::command]
async fn native_context_menu_enabled(webview: tauri::Webview) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        webview
            .with_webview(move |native| {
                let result = (|| -> Result<bool, String> {
                    unsafe {
                        let settings = native
                            .controller()
                            .CoreWebView2()
                            .map_err(error)?
                            .Settings()
                            .map_err(error)?;
                        let mut enabled = Default::default();
                        settings
                            .AreDefaultContextMenusEnabled(&mut enabled)
                            .map_err(error)?;
                        Ok(enabled.as_bool())
                    }
                })();
                let _ = sender.send(result);
            })
            .map_err(error)?;
        tauri::async_runtime::spawn_blocking(move || {
            receiver
                .recv_timeout(std::time::Duration::from_secs(10))
                .map_err(error)?
        })
        .await
        .map_err(error)?
    }
    #[cfg(not(windows))]
    {
        let _ = webview;
        Err("Native context-menu inspection is Windows-only".into())
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("native-context-menu-policy")
                .on_webview_ready(|webview| configure_webview_context_menus(&webview))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            native_context_menu_enabled,
            write_note_clipboard,
            read_note_clipboard,
            open_document,
            session_view,
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
            set_playback_channels,
            panic_release,
            test_note,
            midi_ownership,
            claim_midi,
            transport_status,
            start_monitor,
            stop_monitor,
            monitor_snapshot,
            monitor_held_notes,
            clear_monitor,
            start_capture,
            stop_capture,
            capture_snapshot,
            capture_marker,
            capture_inject_cc,
            clear_capture,
            export_capture_midi,
            export_section_csv,
            export_section_json,
            apply_capture_to_section,
            export_phrase_midi,
            preview_phrase_midi,
            apply_phrase_midi,
            inspect_library_style,
            scan_library_folder,
            preview_library_phrase,
            import_library_phrase,
            import_library_section,
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
            reset_native_drums,
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
            extract_expansion_resource,
            extract_all_expansion_resources,
            rewrite_expansion_ppf
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

#[cfg(test)]
mod exchange_preview_tests {
    use super::*;
    fn session() -> Session {
        let track = vec![
            0, 255, 6, 6, b'M', b'a', b'i', b'n', b' ', b'A', 0, 0x98, 60, 90, 120, 0x88, 60, 0,
            0x82, 0x68, 255, 47, 0,
        ];
        let bytes = genos_core::smf::pack(&[
            genos_core::smf::Chunk {
                tag: *b"MThd",
                data: vec![0, 0, 0, 1, 1, 224],
            },
            genos_core::smf::Chunk {
                tag: *b"MTrk",
                data: track,
            },
        ])
        .unwrap();
        Session::open(&bytes, "Exchange test".into()).unwrap()
    }
    fn studio() -> Studio {
        let mut s = Studio {
            session: Some(session()),
            epoch: 1,
            ..Default::default()
        };
        s.projects.insert(
            "project-2".into(),
            ProjectDoc {
                session: session(),
                epoch: 1,
                exchange_nonce: 0,
                pending_exchange: None,
                pending_live: None,
            },
        );
        s
    }
    fn queue(s: &mut Studio, label: &str) -> String {
        queue_exchange(
            s,
            PendingExchange {
                token: String::new(),
                epoch: doc_epoch(s, label),
                revision: session_ref(s, label).unwrap().revision,
                section: 0,
                channel: 9,
                notes: vec![],
                phrases: vec![],
                doc_label: label.into(),
            },
        )
        .unwrap()
    }
    #[test]
    fn both_windows_keep_their_own_preview_even_with_equal_epochs_revisions_and_counters() {
        let mut s = studio();
        let main = queue(&mut s, "main");
        let secondary = queue(&mut s, "project-2");
        assert_ne!(main, secondary);
        assert_eq!(take_exchange(&mut s, &main).unwrap().doc_label, "main");
        assert_eq!(
            take_exchange(&mut s, &secondary).unwrap().doc_label,
            "project-2"
        );
        assert!(take_exchange(&mut s, &main).is_err());
    }
    #[test]
    fn replacing_one_preview_expires_only_its_own_token() {
        let mut s = studio();
        let old = queue(&mut s, "main");
        let secondary = queue(&mut s, "project-2");
        let latest = queue(&mut s, "main");
        assert!(take_exchange(&mut s, &old).is_err());
        assert_eq!(take_exchange(&mut s, &latest).unwrap().doc_label, "main");
        assert_eq!(
            take_exchange(&mut s, &secondary).unwrap().doc_label,
            "project-2"
        );
    }
    #[test]
    fn stale_revision_epoch_or_closed_project_reject_without_consuming_other_previews() {
        for change in 0..3 {
            let mut s = studio();
            let main = queue(&mut s, "main");
            let secondary = queue(&mut s, "project-2");
            match change {
                0 => s.projects.get_mut("project-2").unwrap().session.revision += 1,
                1 => s.projects.get_mut("project-2").unwrap().epoch += 1,
                _ => {
                    s.projects.remove("project-2");
                }
            }
            assert!(take_exchange(&mut s, &secondary).is_err());
            assert_eq!(take_exchange(&mut s, &main).unwrap().doc_label, "main");
        }
    }
}
