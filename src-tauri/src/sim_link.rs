//! The "simulated treadmill" choice of dev builds: a [`SimDeck`] behind the
//! same seam as the real deck (`bluetooth::DeckLink`), so the whole app runs
//! with no hardware. Dev builds only: `lib.rs` has
//! `#[cfg(debug_assertions)] mod sim_link;`, so release builds never contain it.
//!
//! The deck lives for the app's run, as a real one stays switched on: a
//! reconnect finds the belt as the link left it. Its walks stay in memory
//! (negative session ids), so a dev run never reaches History, the odometer
//! or Strava. Connecting it saves it as the treadmill to reconnect to, in the
//! dev builds' own database.

use crate::sim::SimDeck;
use btleplug::api::ValueNotification;
use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The scan entry's id and name.
pub(crate) const SIM_DECK_ID: &str = "SIM_DECK";
pub(crate) const SIM_DECK_NAME: &str = "SIM_DECK // simulated treadmill";

/// The status characteristic the notifications come from (FitShow `fff1`).
const STATUS_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x0000fff1_0000_1000_8000_00805f9b34fb);
/// How often the notification stream looks for the deck's answers; the deck
/// answers each write 55 ms after it, as the real one does.
const NOTIFY_POLL: Duration = Duration::from_millis(20);

/// The one simulated deck.
static DECK: LazyLock<Arc<Mutex<SimDeck>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SimDeck::new(Instant::now()))));
/// The next in-memory session id; counts down from -1.
static NEXT_SESSION: AtomicI64 = AtomicI64::new(-1);

/// One connection to the simulated deck.
#[derive(Clone)]
pub(crate) struct SimLink {
    deck: Arc<Mutex<SimDeck>>,
    /// Tests only: a disconnect never returns, as one can after the PC sleeps.
    #[cfg(test)]
    hang_disconnect: bool,
}

impl SimLink {
    /// Link up to the app's simulated deck, woken first as someone at the
    /// deck would before connecting. Nothing here moves the belt.
    pub(crate) fn connect() -> Self {
        Self::connect_to(DECK.clone())
    }

    pub(crate) fn connect_to(deck: Arc<Mutex<SimDeck>>) -> Self {
        let link = Self {
            deck,
            #[cfg(test)]
            hang_disconnect: false,
        };
        {
            let mut deck = link.lock();
            deck.wake(now());
            deck.connect();
        }
        link
    }

    fn lock(&self) -> MutexGuard<'_, SimDeck> {
        self.deck.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.lock().linked()
    }

    pub(crate) fn disconnect(&self) {
        self.lock().drop_link();
    }

    /// The same link, but its disconnect hangs (`DeckLink::disconnect`).
    #[cfg(test)]
    pub(crate) fn hanging_disconnect(self) -> Self {
        Self {
            hang_disconnect: true,
            ..self
        }
    }

    #[cfg(test)]
    pub(crate) fn hangs_on_disconnect(&self) -> bool {
        self.hang_disconnect
    }

    /// A packet written to the deck.
    pub(crate) fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let mut deck = self.lock();
        if !deck.linked() {
            return Err("Simulated deck not linked".to_string());
        }
        deck.receive(bytes, now());
        Ok(())
    }

    /// The deck's notifications as the status characteristic delivers them;
    /// the stream ends when the link drops, like a real one.
    pub(crate) fn notifications(&self) -> Pin<Box<dyn Stream<Item = ValueNotification> + Send>> {
        let link = self.clone();
        Box::pin(futures::stream::unfold(
            (link, VecDeque::new()),
            |(link, mut queue)| async move {
                loop {
                    if let Some(value) = queue.pop_front() {
                        let note = ValueNotification {
                            uuid: STATUS_UUID,
                            value,
                        };
                        return Some((note, (link, queue)));
                    }
                    if !link.is_connected() {
                        return None;
                    }
                    tokio::time::sleep(NOTIFY_POLL).await;
                    queue.extend(link.lock().take_notifications(now()));
                }
            },
        ))
    }
}

/// The deck's clock: tokio's, so a test can pause it; outside a paused test
/// it is the wall clock.
fn now() -> Instant {
    tokio::time::Instant::now().into_std()
}

/// An id for a simulated walk's in-memory session.
pub(crate) fn next_session_id() -> i64 {
    NEXT_SESSION.fetch_sub(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth::{apply_telemetry, DeckLink};
    use crate::engine::{DeckAction, OrchestratorState, PacketKind, ProtocolEngine};
    use crate::protocol;
    use futures::StreamExt;

    type Shared = Arc<tokio::sync::Mutex<OrchestratorState>>;

    /// The protocol worker's loop over the seam, as `spawn_protocol_worker`
    /// runs it (no app: no toasts, sessions or ticks), for `secs`.
    async fn work(
        deck: &DeckLink,
        engine: &mut ProtocolEngine,
        state: &Shared,
        secs: u64,
    ) -> Vec<PacketKind> {
        let end = now() + Duration::from_secs(secs);
        let mut sent = vec![];
        while now() < end {
            let t = now();
            let packet = {
                let mut s = state.lock().await;
                s.set_test_now(t);
                engine.determine_next_packet(&mut s, t)
            };
            if let Some(p) = packet {
                deck.write(&p.bytes).await.expect("the sim takes the write");
                state.lock().await.last_cmd_time = t;
                sent.push(p.kind);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        sent
    }

    /// A SimDeck connection through the seam: connecting leaves the belt
    /// still, Start runs it through the countdown, Stop winds it down, and a
    /// dropped link ends the notification stream.
    #[test]
    fn a_sim_connection_drives_a_start_stop_cycle_through_the_engine() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        rt.block_on(async {
            let sim = Arc::new(Mutex::new(SimDeck::new(now())));
            let link = SimLink::connect_to(sim.clone());
            let deck = DeckLink::Sim(link.clone());
            assert!(deck.is_connected().await);
            assert_eq!(deck.id(), SIM_DECK_ID);

            let state: Shared = Default::default();
            {
                let mut s = state.lock().await;
                s.set_test_now(now());
                s.reset_connection();
                s.sync_config(&crate::persistence::AppConfig::default());
            }
            // The status listener, as `bluetooth::listen_for_status` takes frames.
            let listener = {
                let state = state.clone();
                let mut stream = link.notifications();
                tokio::spawn(async move {
                    let mut frames = 0;
                    while let Some(note) = stream.next().await {
                        if let Some(t) = protocol::parse_status(&note.value) {
                            let mut s = state.lock().await;
                            s.set_test_now(now());
                            apply_telemetry(&mut s, &t);
                            s.update_physics();
                            frames += 1;
                        }
                    }
                    frames
                })
            };
            let mut engine = ProtocolEngine::new();
            let status = |sim: &Arc<Mutex<SimDeck>>| sim.lock().unwrap().status_code();

            // Connecting never starts the belt.
            let sent = work(&deck, &mut engine, &state, 3).await;
            assert!(sent.contains(&PacketKind::PollStatus), "{sent:?}");
            assert!(
                !sent.iter().any(|k| matches!(
                    k,
                    PacketKind::HandshakeReady
                        | PacketKind::HandshakeStart
                        | PacketKind::ControlSet
                )),
                "{sent:?}"
            );
            assert_eq!(status(&sim), 0);
            assert!(state.lock().await.hold_auto_start);

            state.lock().await.trigger_action(DeckAction::Start);
            work(&deck, &mut engine, &state, 8).await;
            assert_eq!(status(&sim), 3, "running after the countdown");
            assert!(sim.lock().unwrap().speed_kmh() > 0.0);
            assert_eq!(
                state.lock().await.last_status,
                protocol::MachineStatus::Running
            );

            state.lock().await.trigger_action(DeckAction::Stop);
            work(&deck, &mut engine, &state, 8).await;
            assert!(sim.lock().unwrap().belt_still());
            let history = sim.lock().unwrap().history.clone();
            assert!(history.windows(2).any(|w| w == [2, 3]), "{history:?}");
            assert!(history.contains(&4), "{history:?}");

            deck.disconnect().await;
            assert!(!deck.is_connected().await);
            let frames = listener.await.unwrap();
            assert!(frames > 20, "status frames taken: {frames}");
        });
    }
}
