//! The tray icon's right-click menu: a status line, the belt and program
//! controls the HUD offers, the saved programs, and the windows. It works
//! with the HUD hidden or closed.
//!
//! The menu is a pure model (`build`: what the snapshot says, in; items with
//! their labels and whether they can act, out), and every item that acts on
//! the belt or a program sends the same `Command` the HUD sends, through
//! `snapshot::dispatch`. So every safety rule the HUD and the shortcuts get
//! applies here too, and the tray adds no new way to move the belt. A click
//! is a user press, like a HUD button.
//!
//! The native menu keeps its layout while the state changes: items are
//! enabled, disabled and relabelled in place (`plan`), and the menu is only
//! rebuilt when the layout changes (the saved programs changed). The status
//! lines change at most once per `STATUS_EVERY_MS`.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use super::{AppSnapshot, Command};
use crate::link::LinkState;
use crate::programs::ProgramPhase;
use crate::protocol::MachineStatus;
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

/// The Quit item's id; `lib.rs` handles it, since quitting is wiring.
pub const QUIT_ID: &str = "quit";
/// The status lines change at most this often. The walk's time has 1 s
/// steps, so a faster update shows nothing new; the snapshot ticks every
/// 250 ms, and four Win32 menu edits a second for a clock would be waste.
/// Items that change whether they can act go out on the next tick.
const STATUS_EVERY_MS: u64 = 1000;
/// Longest program name shown, in characters. Windows makes a menu as wide
/// as its widest item; a pasted paragraph would make a menu wider than the
/// screen.
const MAX_NAME_CHARS: usize = 40;
const SEP: &str = " \u{b7} ";

/// What a menu item does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// ENGAGE, STASIS, GO or RE-JACK: the HUD's main button.
    ToggleStart,
    /// DUMP.
    Stop,
    SpeedUp,
    SpeedDown,
    InclineUp,
    InclineDown,
    /// GO on a paused or waiting program.
    ProgramGo,
    /// HOLD on a walking program.
    ProgramHold,
    ProgramSkip,
    ProgramEnd,
    /// Run a saved program, as the Programs page and the HUD do.
    RunProgram(u32),
    OpenDashboard,
    /// Show the HUD, or hide it.
    ToggleHud,
    Quit,
}

impl TrayAction {
    /// The command the HUD sends for the same button. `None` for the window
    /// items, which move nothing.
    pub fn command(self) -> Option<Command> {
        Some(match self {
            TrayAction::ToggleStart => Command::ToggleStart,
            TrayAction::Stop => Command::Stop,
            TrayAction::SpeedUp => Command::NudgeSpeed { up: true },
            TrayAction::SpeedDown => Command::NudgeSpeed { up: false },
            TrayAction::InclineUp => Command::NudgeIncline { up: true },
            TrayAction::InclineDown => Command::NudgeIncline { up: false },
            TrayAction::ProgramGo => Command::ProgramResume,
            TrayAction::ProgramHold => Command::ProgramPause,
            TrayAction::ProgramSkip => Command::ProgramSkip,
            TrayAction::ProgramEnd => Command::ProgramEnd,
            TrayAction::RunProgram(id) => Command::ProgramRun { id, minutes: None },
            TrayAction::OpenDashboard | TrayAction::ToggleHud | TrayAction::Quit => return None,
        })
    }
}

/// One menu entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Item(Item),
    Separator,
    Submenu {
        id: String,
        label: String,
        items: Vec<Item>,
    },
}

/// One clickable (or greyed out) line.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Stable across states, so the native item can be updated in place.
    pub id: String,
    /// As shown, with `&` doubled (a single one marks a mnemonic).
    pub label: String,
    pub enabled: bool,
    /// `None` for the status lines.
    pub action: Option<TrayAction>,
}

impl Item {
    fn status(id: &str, label: String) -> Self {
        Self {
            id: id.to_string(),
            label,
            enabled: false,
            action: None,
        }
    }

    fn act(id: &str, label: &str, enabled: bool, action: TrayAction) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            enabled,
            action: Some(action),
        }
    }
}

/// The running program, as the menu needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramView {
    pub id: u32,
    pub name: String,
    pub resting: bool,
    pub paused: bool,
    /// Paused waiting for GO (`ActiveProgram::wait`).
    pub waiting: bool,
    /// 1-based.
    pub step: u32,
    pub steps: u32,
    /// The step's kind and count within it, e.g. `INTERVAL 3/5`
    /// (`StepKindSpot::label`); None for a step with no kind.
    pub kind_label: Option<String>,
    pub seconds_remaining: Option<u32>,
    /// A cool-down past its minutes walks on until the pulse is down to this
    /// (`ActiveProgram::pulse_to_bpm`), shown in place of the time.
    pub pulse_to_bpm: Option<u32>,
}

/// What the menu is built from: the snapshot, plus whether the HUD shows.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuInput {
    pub connected: bool,
    pub link: LinkState,
    pub status: MachineStatus,
    pub unknown_status_code: Option<u8>,
    pub standby: bool,
    pub start_pending: bool,
    pub speed_kmh: f32,
    pub target_speed_kmh: f32,
    pub min_speed_kmh: f32,
    pub max_speed_kmh: f32,
    pub target_incline_level: u8,
    pub max_incline_level: u8,
    /// The open session's time, if a session is open.
    pub session_s: Option<u64>,
    pub program: Option<ProgramView>,
    /// The saved programs, in the Programs page's order.
    pub programs: Vec<(u32, String)>,
    pub hud_shown: bool,
}

impl MenuInput {
    pub fn from_snapshot(s: &AppSnapshot, hud_shown: bool) -> Self {
        Self {
            connected: s.connection.connected,
            link: s.connection.link.clone(),
            status: s.deck.status,
            unknown_status_code: s.deck.unknown_status_code,
            standby: s.deck.standby,
            start_pending: s.connection.start_pending,
            speed_kmh: s.deck.speed_kmh,
            target_speed_kmh: s.deck.target_speed_kmh,
            min_speed_kmh: s.deck.min_speed_kmh,
            max_speed_kmh: s.deck.max_speed_kmh,
            target_incline_level: s.deck.target_incline_level,
            max_incline_level: s.deck.max_incline_level,
            session_s: s.session.active_id.map(|_| s.session.duration_s),
            program: s.active_program.as_ref().map(|a| ProgramView {
                id: a.program_id,
                name: a.name.clone(),
                resting: a.phase == ProgramPhase::Resting,
                paused: a.paused,
                waiting: a.wait.is_some(),
                step: a.step_index + 1,
                steps: a.step_count,
                kind_label: a.kind.as_ref().map(|k| k.label.clone()),
                seconds_remaining: a.seconds_remaining,
                pulse_to_bpm: a.pulse_to_bpm,
            }),
            programs: s.programs.iter().map(|p| (p.id, p.name.clone())).collect(),
            hud_shown,
        }
    }

    /// Before anything is known: the menu the tray starts with.
    pub fn starting() -> Self {
        Self {
            connected: false,
            link: LinkState::Idle,
            status: MachineStatus::Stopped,
            unknown_status_code: None,
            standby: false,
            start_pending: false,
            speed_kmh: 0.0,
            target_speed_kmh: 0.0,
            min_speed_kmh: 0.0,
            max_speed_kmh: 0.0,
            target_incline_level: 0,
            max_incline_level: 0,
            session_s: None,
            program: None,
            programs: Vec::new(),
            hud_shown: false,
        }
    }

    fn moving(&self) -> bool {
        matches!(
            self.status,
            MachineStatus::Running | MachineStatus::Starting
        )
    }
}

/// Speeds closer than this count as the same (calibrated km/h); a deck step
/// is never smaller than about 0.08.
const SPEED_EPS: f32 = 0.04;

/// The link word, as the HUD header shows it (`linkView().hudLabel`).
fn link_label(link: &LinkState) -> String {
    match link {
        LinkState::Scanning => "SCANNING".into(),
        LinkState::Connecting { attempt, of } => format!("LINKING {attempt}/{of}"),
        LinkState::Reconnecting { next_retry_s, .. } => format!("RELINK {next_retry_s}S"),
        LinkState::AdapterUnavailable => "BT OFFLINE".into(),
        LinkState::Failed { .. } => "LINK FAILED".into(),
        LinkState::NoData { .. } => "NO SIGNAL".into(),
        LinkState::Idle | LinkState::Connected => "LINK DOWN".into(),
    }
}

fn with_time(text: &str, session_s: Option<u64>) -> String {
    match session_s {
        Some(s) => format!("{text}{SEP}{}", crate::util::format_duration(s as i64)),
        None => text.to_string(),
    }
}

/// The first line: the link, or what the belt does and the walk's time,
/// in the HUD header's words. `VEL 3.2 KM/H · 12:34`, `STASIS · 12:34`,
/// `DECK OFF`, `RELINK 12S`.
pub fn status_line(v: &MenuInput) -> String {
    if !v.connected {
        return link_label(&v.link);
    }
    if matches!(v.link, LinkState::NoData { .. }) {
        return "NO SIGNAL".into();
    }
    if let Some(code) = v.unknown_status_code {
        return format!("STATE ?{code}");
    }
    if v.standby {
        return "DECK OFF".into();
    }
    match v.status {
        MachineStatus::Running => with_time(&format!("VEL {:.1} KM/H", v.speed_kmh), v.session_s),
        MachineStatus::Starting => with_time("SPIN-UP", v.session_s),
        MachineStatus::Paused => with_time("STASIS", v.session_s),
        MachineStatus::Stopping => "SPIN-DOWN".into(),
        MachineStatus::Stopped => "STANDBY".into(),
    }
}

/// The second line: the running program and its step, or none.
pub fn program_line(v: &MenuInput) -> String {
    let Some(p) = &v.program else {
        return "NO PROGRAM".into();
    };
    let what = if p.waiting {
        "READY"
    } else if p.paused {
        "PAUSED"
    } else if p.resting {
        "REST"
    } else {
        "PROGRAM"
    };
    let mut text = format!("{what} {}", menu_text(&p.name));
    if !p.resting {
        if let Some(kind) = &p.kind_label {
            text.push_str(SEP);
            text.push_str(kind);
        }
        if p.kind_label.is_none() || p.steps > 1 {
            text.push_str(&format!("{SEP}STEP {}/{}", p.step, p.steps));
        }
    }
    if let Some(s) = p.seconds_remaining {
        text.push_str(SEP);
        text.push_str(&crate::util::format_duration(s as i64));
    }
    if let Some(bpm) = p.pulse_to_bpm {
        text.push_str(&format!("{SEP}PULSE TO {bpm}"));
    }
    text
}

/// The main button's word, as on the HUD (`engageLabel`).
fn engage_label(v: &MenuInput) -> &'static str {
    if v.moving() {
        return "STASIS";
    }
    if v.start_pending {
        return "WAKING\u{2026}";
    }
    if v.program.as_ref().is_some_and(|p| !p.resting && p.paused) {
        return "GO";
    }
    if v.status == MachineStatus::Paused {
        "RE-JACK"
    } else {
        "ENGAGE"
    }
}

/// A saved name as menu text: trimmed, shortened, `&` shown as itself.
fn menu_text(name: &str) -> String {
    let name = name.trim();
    let short: String = if name.chars().count() > MAX_NAME_CHARS {
        let cut: String = name.chars().take(MAX_NAME_CHARS - 1).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        name.to_string()
    };
    short.replace('&', "&&")
}

/// The menu for a state. Every section is always there; an item that can't
/// act now is greyed out, never hidden, so the layout stays put.
pub fn build(v: &MenuInput) -> Vec<Node> {
    let c = v.connected;
    let program = v.program.as_ref();
    let mut nodes = vec![
        Node::Item(Item::status("status", status_line(v))),
        Node::Item(Item::status("program_status", program_line(v))),
        Node::Separator,
        // The HUD's transport and steppers, enabled as there, plus nothing to
        // do at a limit.
        Node::Item(Item::act(
            "engage",
            engage_label(v),
            c,
            TrayAction::ToggleStart,
        )),
        Node::Item(Item::act(
            "dump",
            "DUMP",
            c && !matches!(v.status, MachineStatus::Stopped | MachineStatus::Stopping),
            TrayAction::Stop,
        )),
        Node::Item(Item::act(
            "vel_up",
            "VEL UP",
            c && v.target_speed_kmh < v.max_speed_kmh - SPEED_EPS,
            TrayAction::SpeedUp,
        )),
        Node::Item(Item::act(
            "vel_down",
            "VEL DOWN",
            c && v.target_speed_kmh > v.min_speed_kmh + SPEED_EPS,
            TrayAction::SpeedDown,
        )),
        Node::Item(Item::act(
            "tilt_up",
            "TILT UP",
            c && v.target_incline_level < v.max_incline_level,
            TrayAction::InclineUp,
        )),
        Node::Item(Item::act(
            "tilt_down",
            "TILT DOWN",
            c && v.target_incline_level > 0,
            TrayAction::InclineDown,
        )),
        Node::Separator,
    ];
    // The HUD's program row: GO on a paused program, HOLD on a walking one.
    let paused = program.is_some_and(|p| p.paused);
    let (go_label, go_action) = if paused {
        ("GO", TrayAction::ProgramGo)
    } else {
        ("HOLD", TrayAction::ProgramHold)
    };
    nodes.push(Node::Item(Item::act(
        "program_go",
        go_label,
        c && program.is_some(),
        go_action,
    )));
    nodes.push(Node::Item(Item::act(
        "program_skip",
        "SKIP",
        c && program.is_some(),
        TrayAction::ProgramSkip,
    )));
    // END never moves the belt; it works while the link is down too.
    nodes.push(Node::Item(Item::act(
        "program_end",
        "END",
        program.is_some(),
        TrayAction::ProgramEnd,
    )));
    let items = if v.programs.is_empty() {
        vec![Item::status("programs_none", "No programs saved".into())]
    } else {
        v.programs
            .iter()
            .map(|(id, name)| {
                // The running one is greyed out, as on the Programs page.
                let running = program.is_some_and(|p| p.id == *id);
                let label = if running {
                    format!("{}{SEP}RUNNING", menu_text(name))
                } else {
                    menu_text(name)
                };
                Item {
                    id: format!("program:{id}"),
                    label,
                    enabled: c && !running,
                    action: Some(TrayAction::RunProgram(*id)),
                }
            })
            .collect()
    };
    nodes.push(Node::Submenu {
        id: "programs".into(),
        label: "RUN PROGRAM".into(),
        items,
    });
    nodes.push(Node::Separator);
    nodes.push(Node::Item(Item::act(
        "dashboard",
        "Open dashboard",
        true,
        TrayAction::OpenDashboard,
    )));
    nodes.push(Node::Item(Item::act(
        "hud",
        if v.hud_shown { "Hide HUD" } else { "Show HUD" },
        true,
        TrayAction::ToggleHud,
    )));
    nodes.push(Node::Item(Item::act(
        QUIT_ID,
        "Quit",
        true,
        TrayAction::Quit,
    )));
    nodes
}

/// The menu as an indented text tree (`[x]` greyed out), for tests and logs.
pub fn render_tree(nodes: &[Node]) -> String {
    fn line(out: &mut String, depth: usize, item: &Item) {
        let mark = if item.enabled { "" } else { " [x]" };
        out.push_str(&format!(
            "{}{}{}\n",
            "  ".repeat(depth),
            item.label.replace("&&", "&"),
            mark
        ));
    }
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Item(item) => line(&mut out, 0, item),
            Node::Separator => out.push_str("----\n"),
            Node::Submenu { label, items, .. } => {
                out.push_str(&format!("{label} >\n"));
                for item in items {
                    line(&mut out, 1, item);
                }
            }
        }
    }
    out
}

fn items(nodes: &[Node]) -> impl Iterator<Item = &Item> {
    nodes.iter().flat_map(|n| match n {
        Node::Item(i) => std::slice::from_ref(i).iter(),
        Node::Submenu { items, .. } => items.iter(),
        Node::Separator => [].iter(),
    })
}

/// The layout: which entries, in which order. Labels and enabled states
/// are not part of it.
fn layout(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|n| match n {
            Node::Item(i) => vec![i.id.clone()],
            Node::Separator => vec!["-".into()],
            Node::Submenu { id, items, .. } => std::iter::once(format!("{id}>"))
                .chain(items.iter().map(|i| i.id.clone()))
                .chain(std::iter::once("<".into()))
                .collect(),
        })
        .collect()
}

/// What to do to the native menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    Nothing,
    /// The layout changed (or there is no menu yet): build it afresh.
    Rebuild,
    /// Same layout: change these items in place.
    Update(Vec<Item>),
}

/// How to go from the menu shown (`shown`, last changed at `changed_at_ms`)
/// to `next`. Items that change what they can do go out at once; a change
/// to the status lines alone waits until `STATUS_EVERY_MS` has passed.
pub fn plan(shown: Option<&[Node]>, next: &[Node], changed_at_ms: u64, now_ms: u64) -> Plan {
    let Some(shown) = shown else {
        return Plan::Rebuild;
    };
    if layout(shown) != layout(next) {
        return Plan::Rebuild;
    }
    let changed: Vec<Item> = items(shown)
        .zip(items(next))
        .filter(|(a, b)| a != b)
        .map(|(_, b)| b.clone())
        .collect();
    if changed.is_empty() {
        return Plan::Nothing;
    }
    let only_status = changed.iter().all(|i| i.action.is_none());
    if only_status && now_ms.saturating_sub(changed_at_ms) < STATUS_EVERY_MS {
        return Plan::Nothing;
    }
    Plan::Update(changed)
}

/// What each item id does, as last shown. A click does what its label said
/// when the menu was shown (GO, not HOLD, if it read GO), and the command it
/// sends is then judged by Rust against the state at that moment.
static ACTIONS: Mutex<Option<HashMap<String, TrayAction>>> = Mutex::new(None);

fn remember_actions(nodes: &[Node]) {
    let map = items(nodes)
        .filter_map(|i| i.action.map(|a| (i.id.clone(), a)))
        .collect();
    *ACTIONS.lock().unwrap_or_else(PoisonError::into_inner) = Some(map);
}

fn action_for(id: &str) -> Option<TrayAction> {
    ACTIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|m| m.get(id).copied())
}

/// The native items by id, to change in place.
type Handles<R> = HashMap<String, MenuItem<R>>;

/// Build the native menu for `nodes`.
fn native<R: Runtime, M: Manager<R>>(
    manager: &M,
    nodes: &[Node],
) -> tauri::Result<(Menu<R>, Handles<R>)> {
    let menu = Menu::new(manager)?;
    let mut handles = HashMap::new();
    let item =
        |i: &Item| MenuItem::with_id(manager, i.id.as_str(), &i.label, i.enabled, None::<&str>);
    for node in nodes {
        match node {
            Node::Item(i) => {
                let native = item(i)?;
                menu.append(&native)?;
                handles.insert(i.id.clone(), native);
            }
            Node::Separator => menu.append(&PredefinedMenuItem::separator(manager)?)?,
            Node::Submenu { id, label, items } => {
                let sub = Submenu::with_id(manager, id.as_str(), label, true)?;
                for i in items {
                    let native = item(i)?;
                    sub.append(&native as &dyn IsMenuItem<R>)?;
                    handles.insert(i.id.clone(), native);
                }
                menu.append(&sub)?;
            }
        }
    }
    remember_actions(nodes);
    Ok((menu, handles))
}

/// The menu the tray is built with, before the first snapshot.
pub fn initial_menu<R: Runtime, M: Manager<R>>(manager: &M) -> tauri::Result<Menu<R>> {
    native(manager, &build(&MenuInput::starting())).map(|(menu, _)| menu)
}

/// Keeps the tray menu in step with the snapshots.
#[derive(Default)]
pub struct Updater {
    shown: Option<Vec<Node>>,
    /// The native items by id, to change in place.
    handles: Handles<tauri::Wry>,
    changed_at_ms: u64,
}

impl Updater {
    pub fn apply(&mut self, app: &AppHandle, s: &AppSnapshot, now_ms: u64) {
        let next = build(&MenuInput::from_snapshot(s, crate::hud::shown(app)));
        match plan(self.shown.as_deref(), &next, self.changed_at_ms, now_ms) {
            Plan::Nothing => return,
            Plan::Rebuild => {
                let Some(tray) = app.tray_by_id(super::tray::TRAY_ID) else {
                    return;
                };
                match native(app, &next) {
                    Ok((menu, handles)) => {
                        if let Err(e) = tray.set_menu(Some(menu)) {
                            log::debug!("Tray menu not set: {}", e);
                            return;
                        }
                        self.handles = handles;
                    }
                    Err(e) => {
                        log::debug!("Tray menu not built: {}", e);
                        return;
                    }
                }
            }
            Plan::Update(changed) => {
                for i in &changed {
                    let Some(native) = self.handles.get(&i.id) else {
                        continue;
                    };
                    if let Err(e) = native
                        .set_text(&i.label)
                        .and_then(|_| native.set_enabled(i.enabled))
                    {
                        log::debug!("Tray menu item {} not updated: {}", i.id, e);
                    }
                }
                remember_actions(&next);
            }
        }
        self.shown = Some(next);
        self.changed_at_ms = now_ms;
    }
}

/// A menu item was clicked (Quit is handled by `lib.rs`). The work runs off
/// the menu's thread: a window may be created, which must not happen inside
/// the event handler.
pub fn on_menu_event(app: &AppHandle, id: &str) {
    let Some(action) = action_for(id) else {
        log::debug!("Tray menu: no action for {id}");
        return;
    };
    log::info!("Tray menu: {:?}", action);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match action {
            TrayAction::OpenDashboard => crate::dashboard::show_dashboard(&app),
            TrayAction::ToggleHud => {
                let show = !crate::hud::shown(&app);
                crate::hud::set_shown(&app, show);
            }
            TrayAction::Quit => {}
            other => {
                let Some(command) = other.command() else {
                    return;
                };
                if let Err(e) = super::dispatch(app.clone(), command).await {
                    report_failure(&app, other, &e);
                }
            }
        }
    });
}

/// The tray can't show text under its items. A failure is logged, and shown
/// as a HUD toast while the HUD shows. The app sends no OS notifications, so
/// none is added for this.
fn report_failure(app: &AppHandle, action: TrayAction, error: &str) {
    log::warn!("Tray menu {:?} failed: {}", action, error);
    if crate::hud::shown(app) {
        crate::hud::toast(app, serde_json::json!({ "type": "TRAY", "message": error }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still() -> MenuInput {
        MenuInput {
            connected: true,
            link: LinkState::Connected,
            status: MachineStatus::Stopped,
            unknown_status_code: None,
            standby: false,
            start_pending: false,
            speed_kmh: 0.0,
            target_speed_kmh: 3.0,
            min_speed_kmh: 0.6,
            max_speed_kmh: 6.0,
            target_incline_level: 0,
            max_incline_level: 12,
            session_s: None,
            program: None,
            programs: vec![(1, "Workday".into()), (2, "Climb & Cruise".into())],
            hud_shown: true,
        }
    }

    fn disconnected() -> MenuInput {
        MenuInput {
            connected: false,
            link: LinkState::Reconnecting {
                attempt: 2,
                next_retry_s: 12,
            },
            ..still()
        }
    }

    fn walking() -> MenuInput {
        MenuInput {
            status: MachineStatus::Running,
            speed_kmh: 3.2,
            target_speed_kmh: 3.2,
            target_incline_level: 2,
            session_s: Some(754),
            ..still()
        }
    }

    fn program() -> MenuInput {
        MenuInput {
            program: Some(ProgramView {
                id: 1,
                name: "Workday".into(),
                resting: false,
                paused: false,
                waiting: false,
                step: 2,
                steps: 4,
                kind_label: None,
                seconds_remaining: Some(190),
                pulse_to_bpm: None,
            }),
            ..walking()
        }
    }

    fn find<'a>(nodes: &'a [Node], id: &str) -> &'a Item {
        items(nodes).find(|i| i.id == id).expect(id)
    }

    /// Prints each state's menu (`cargo test tray_menu -- --nocapture`), and
    /// pins it.
    #[test]
    fn menu_tree_per_state() {
        let states = [
            ("disconnected", disconnected()),
            ("still", still()),
            ("walking", walking()),
            ("program running", program()),
        ];
        for (name, v) in &states {
            println!("== {name} ==\n{}", render_tree(&build(v)));
        }
        assert_eq!(
            render_tree(&build(&disconnected())),
            "RELINK 12S [x]\nNO PROGRAM [x]\n----\nENGAGE [x]\nDUMP [x]\nVEL UP [x]\n\
             VEL DOWN [x]\nTILT UP [x]\nTILT DOWN [x]\n----\nHOLD [x]\nSKIP [x]\nEND [x]\n\
             RUN PROGRAM >\n  Workday [x]\n  Climb & Cruise [x]\n----\nOpen dashboard\n\
             Hide HUD\nQuit\n"
        );
        assert_eq!(
            render_tree(&build(&program())),
            "VEL 3.2 KM/H \u{b7} 12:34 [x]\nPROGRAM Workday \u{b7} STEP 2/4 \u{b7} 3:10 [x]\n\
             ----\nSTASIS\nDUMP\nVEL UP\nVEL DOWN\nTILT UP\nTILT DOWN\n----\nHOLD\nSKIP\nEND\n\
             RUN PROGRAM >\n  Workday \u{b7} RUNNING [x]\n  Climb & Cruise\n----\n\
             Open dashboard\nHide HUD\nQuit\n"
        );
    }

    #[test]
    fn status_line_speaks_like_the_hud() {
        assert_eq!(status_line(&walking()), "VEL 3.2 KM/H \u{b7} 12:34");
        assert_eq!(status_line(&still()), "STANDBY");
        assert_eq!(
            status_line(&MenuInput {
                standby: true,
                ..still()
            }),
            "DECK OFF"
        );
        assert_eq!(
            status_line(&MenuInput {
                status: MachineStatus::Paused,
                ..walking()
            }),
            "STASIS \u{b7} 12:34"
        );
        assert_eq!(
            status_line(&MenuInput {
                unknown_status_code: Some(9),
                ..still()
            }),
            "STATE ?9"
        );
        assert_eq!(status_line(&disconnected()), "RELINK 12S");
        let down = |link| {
            status_line(&MenuInput {
                link,
                ..disconnected()
            })
        };
        assert_eq!(down(LinkState::Idle), "LINK DOWN");
        assert_eq!(down(LinkState::AdapterUnavailable), "BT OFFLINE");
        assert_eq!(
            down(LinkState::Connecting { attempt: 1, of: 3 }),
            "LINKING 1/3"
        );
        assert_eq!(
            status_line(&MenuInput {
                link: LinkState::NoData { since_s: 20 },
                ..walking()
            }),
            "NO SIGNAL"
        );
        // An hour in, the time has hours.
        assert_eq!(
            status_line(&MenuInput {
                session_s: Some(3723),
                ..walking()
            }),
            "VEL 3.2 KM/H \u{b7} 1:02:03"
        );
    }

    #[test]
    fn disconnected_greys_out_every_control_but_keeps_them() {
        let nodes = build(&disconnected());
        for id in [
            "engage",
            "dump",
            "vel_up",
            "vel_down",
            "tilt_up",
            "tilt_down",
            "program_go",
            "program_skip",
            "program_end",
            "program:1",
            "program:2",
        ] {
            assert!(!find(&nodes, id).enabled, "{id}");
        }
        for id in ["dashboard", "hud", QUIT_ID] {
            assert!(find(&nodes, id).enabled, "{id}");
        }
        // Same layout as connected: the sections never come and go.
        assert_eq!(layout(&nodes), layout(&build(&program())));
    }

    #[test]
    fn still_belt_offers_engage_and_programs_not_dump() {
        let nodes = build(&still());
        assert_eq!(find(&nodes, "engage").label, "ENGAGE");
        assert!(find(&nodes, "engage").enabled);
        assert!(!find(&nodes, "dump").enabled);
        assert!(find(&nodes, "program:1").enabled);
        assert!(find(&nodes, "program:2").enabled);
        // No program: its controls are there, greyed out.
        assert!(!find(&nodes, "program_go").enabled);
        assert!(!find(&nodes, "program_end").enabled);
        // At flat, TILT DOWN has nothing to do.
        assert!(!find(&nodes, "tilt_down").enabled);
        assert!(find(&nodes, "tilt_up").enabled);
        let paused = build(&MenuInput {
            status: MachineStatus::Paused,
            ..still()
        });
        assert_eq!(find(&paused, "engage").label, "RE-JACK");
        assert!(find(&paused, "dump").enabled);
        let waking = build(&MenuInput {
            start_pending: true,
            ..still()
        });
        assert_eq!(find(&waking, "engage").label, "WAKING\u{2026}");
    }

    #[test]
    fn steppers_grey_out_at_their_limits() {
        let top = build(&MenuInput {
            target_speed_kmh: 6.0,
            target_incline_level: 12,
            ..walking()
        });
        assert!(!find(&top, "vel_up").enabled);
        assert!(find(&top, "vel_down").enabled);
        assert!(!find(&top, "tilt_up").enabled);
        assert!(find(&top, "tilt_down").enabled);
        let floor = build(&MenuInput {
            target_speed_kmh: 0.6,
            ..walking()
        });
        assert!(!find(&floor, "vel_down").enabled);
        assert!(find(&floor, "vel_up").enabled);
    }

    #[test]
    fn program_controls_follow_the_program() {
        let nodes = build(&program());
        assert_eq!(find(&nodes, "program_go").label, "HOLD");
        assert_eq!(
            find(&nodes, "program_go").action,
            Some(TrayAction::ProgramHold)
        );
        assert!(find(&nodes, "program_skip").enabled);
        assert!(find(&nodes, "program_end").enabled);
        // The running program can't be started again; the others can.
        assert!(!find(&nodes, "program:1").enabled);
        assert!(find(&nodes, "program:2").enabled);

        // Waiting for GO: both GO buttons read GO, as on the HUD.
        let mut waiting = program();
        waiting.status = MachineStatus::Paused;
        if let Some(p) = waiting.program.as_mut() {
            p.paused = true;
            p.waiting = true;
            p.seconds_remaining = None;
        }
        let nodes = build(&waiting);
        assert_eq!(find(&nodes, "program_go").label, "GO");
        assert_eq!(
            find(&nodes, "program_go").action,
            Some(TrayAction::ProgramGo)
        );
        assert_eq!(find(&nodes, "engage").label, "GO");
        assert_eq!(program_line(&waiting), "READY Workday \u{b7} STEP 2/4");

        // A cool-down the pulse keeps going names the pulse in place of the time.
        let mut cooling = program();
        if let Some(p) = cooling.program.as_mut() {
            p.kind_label = Some("COOL_DOWN".to_string());
            p.seconds_remaining = None;
            p.pulse_to_bpm = Some(114);
        }
        assert_eq!(
            program_line(&cooling),
            "PROGRAM Workday \u{b7} COOL_DOWN \u{b7} STEP 2/4 \u{b7} PULSE TO 114"
        );

        // END works with the link down; GO and SKIP wait for it.
        let mut offline = program();
        offline.connected = false;
        let nodes = build(&offline);
        assert!(find(&nodes, "program_end").enabled);
        assert!(!find(&nodes, "program_go").enabled);
        assert!(!find(&nodes, "program_skip").enabled);

        // Resting between cycles.
        let mut resting = program();
        if let Some(p) = resting.program.as_mut() {
            p.resting = true;
        }
        assert_eq!(program_line(&resting), "REST Workday \u{b7} 3:10");
    }

    /// A step with a kind shows it before the step number; a one-step
    /// program with a kind drops the 1/1. Without a kind, as before #235.
    #[test]
    fn program_line_shows_the_step_kind() {
        let mut v = program();
        if let Some(p) = v.program.as_mut() {
            p.kind_label = Some("INTERVAL 1/2".into());
        }
        assert_eq!(
            program_line(&v),
            "PROGRAM Workday \u{b7} INTERVAL 1/2 \u{b7} STEP 2/4 \u{b7} 3:10"
        );
        if let Some(p) = v.program.as_mut() {
            p.kind_label = Some("WALK".into());
            p.step = 1;
            p.steps = 1;
        }
        assert_eq!(program_line(&v), "PROGRAM Workday \u{b7} WALK \u{b7} 3:10");
        if let Some(p) = v.program.as_mut() {
            p.kind_label = None;
        }
        assert_eq!(
            program_line(&v),
            "PROGRAM Workday \u{b7} STEP 1/1 \u{b7} 3:10"
        );
    }

    #[test]
    fn each_item_sends_the_huds_command() {
        let expected = [
            ("engage", Command::ToggleStart),
            ("dump", Command::Stop),
            ("vel_up", Command::NudgeSpeed { up: true }),
            ("vel_down", Command::NudgeSpeed { up: false }),
            ("tilt_up", Command::NudgeIncline { up: true }),
            ("tilt_down", Command::NudgeIncline { up: false }),
            ("program_go", Command::ProgramPause),
            ("program_skip", Command::ProgramSkip),
            ("program_end", Command::ProgramEnd),
            (
                "program:2",
                Command::ProgramRun {
                    id: 2,
                    minutes: None,
                },
            ),
        ];
        let nodes = build(&program());
        for (id, command) in expected {
            let action = find(&nodes, id).action.expect(id);
            assert_eq!(action.command(), Some(command), "{id}");
        }
        assert_eq!(
            TrayAction::ProgramGo.command(),
            Some(Command::ProgramResume)
        );
        // The window items send nothing to the treadmill.
        for id in ["dashboard", "hud", QUIT_ID] {
            assert_eq!(find(&nodes, id).action.unwrap().command(), None, "{id}");
        }
        // The status lines do nothing.
        assert_eq!(find(&nodes, "status").action, None);
        assert_eq!(find(&nodes, "program_status").action, None);
    }

    /// Every command the menu can send is one the HUD's buttons already send
    /// (HudPanel.tsx: the transport, steppers and program row): nothing here
    /// is a new way to move the belt.
    #[test]
    fn the_menu_sends_only_commands_the_hud_sends() {
        let hud = [
            Command::ToggleStart,
            Command::Stop,
            Command::NudgeSpeed { up: true },
            Command::NudgeSpeed { up: false },
            Command::NudgeIncline { up: true },
            Command::NudgeIncline { up: false },
            Command::ProgramResume,
            Command::ProgramPause,
            Command::ProgramSkip,
            Command::ProgramEnd,
        ];
        let mut inputs = vec![disconnected(), still(), walking(), program()];
        let mut paused = program();
        if let Some(p) = paused.program.as_mut() {
            p.paused = true;
        }
        inputs.push(paused);
        for v in &inputs {
            for item in items(&build(v)) {
                match item.action.and_then(TrayAction::command) {
                    None => {}
                    Some(Command::ProgramRun { id, .. }) => {
                        assert!(v.programs.iter().any(|(p, _)| *p == id))
                    }
                    Some(c) => assert!(hud.contains(&c), "{c:?}"),
                }
            }
        }
    }

    #[test]
    fn names_are_kept_short_and_ampersands_show() {
        assert_eq!(menu_text("  Climb & Cruise "), "Climb && Cruise");
        let long = menu_text(&"x".repeat(100));
        assert_eq!(long.chars().count(), MAX_NAME_CHARS);
        assert!(long.ends_with('\u{2026}'));
        let none = build(&MenuInput {
            programs: Vec::new(),
            ..still()
        });
        let empty = find(&none, "programs_none");
        assert_eq!(empty.label, "No programs saved");
        assert!(!empty.enabled);
        assert_eq!(empty.action, None);
    }

    #[test]
    fn hud_item_says_what_it_will_do() {
        let shown = build(&still());
        assert_eq!(find(&shown, "hud").label, "Hide HUD");
        let hidden = build(&MenuInput {
            hud_shown: false,
            ..still()
        });
        assert_eq!(find(&hidden, "hud").label, "Show HUD");
    }

    #[test]
    fn plan_updates_in_place_and_throttles_the_clock() {
        let a = build(&walking());
        assert_eq!(plan(None, &a, 0, 0), Plan::Rebuild);
        assert_eq!(plan(Some(&a), &a, 0, 5000), Plan::Nothing);

        // Only the time moved: held back under a second, then sent.
        let b = build(&MenuInput {
            session_s: Some(755),
            ..walking()
        });
        assert_eq!(plan(Some(&a), &b, 1000, 1250), Plan::Nothing);
        match plan(Some(&a), &b, 1000, 2000) {
            Plan::Update(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, "status");
            }
            other => panic!("{other:?}"),
        }

        // What an item can do changed: at once, on the next tick.
        let stopped = build(&still());
        match plan(Some(&a), &stopped, 1000, 1250) {
            Plan::Update(items) => {
                assert!(items
                    .iter()
                    .any(|i| i.id == "engage" && i.label == "ENGAGE"));
                assert!(items.iter().any(|i| i.id == "dump" && !i.enabled));
            }
            other => panic!("{other:?}"),
        }

        // A program saved or deleted changes the layout: rebuilt.
        let more = build(&MenuInput {
            programs: vec![(1, "Workday".into())],
            ..walking()
        });
        assert_eq!(plan(Some(&a), &more, 1000, 1250), Plan::Rebuild);
        // A renamed one doesn't.
        let renamed = build(&MenuInput {
            programs: vec![(1, "Desk day".into()), (2, "Climb & Cruise".into())],
            ..walking()
        });
        assert!(matches!(
            plan(Some(&a), &renamed, 1000, 1250),
            Plan::Update(_)
        ));
    }

    #[test]
    fn a_click_does_what_the_label_said() {
        remember_actions(&build(&program()));
        assert_eq!(action_for("program_go"), Some(TrayAction::ProgramHold));
        assert_eq!(action_for("program:2"), Some(TrayAction::RunProgram(2)));
        assert_eq!(action_for("status"), None);
        assert_eq!(action_for("nonsense"), None);
    }
}
