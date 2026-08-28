use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, info, warn};

// ============================================================================
// Task 30: Configurable constants for steal / kill-retention detection windows
// These are used as defaults in EventStreamConfig; at runtime the actual values
// come from self.config.contest_window_secs and self.config.kill_retention_secs.
// ============================================================================

/// Default window (seconds) within which an enemy kill near an objective counts as "contested"
const DEFAULT_CONTEST_WINDOW_SECS: u64 = 10;

/// Default window (seconds) within which champion kills are retained for steal detection
const DEFAULT_KILL_RETENTION_SECS: u64 = 15;

// ============================================================================
// Task 29: Game result inference
// ============================================================================

/// Inferred result of a game session based on game duration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GameResult {
    Victory,
    Defeat,
    /// Surrender before 20 minutes but after remake threshold
    EarlySurrender,
    /// Game ended before 5 minutes (remake)
    Remake,
    Unknown,
}

/// Infer game result from total game duration in seconds.
/// Victory/Defeat cannot be determined from duration alone — those require
/// additional data from the Live Client API (not available at GameEnd event time).
fn infer_game_result(game_time_secs: f64) -> GameResult {
    if game_time_secs < 300.0 {
        GameResult::Remake
    } else if game_time_secs < 1200.0 {
        GameResult::EarlySurrender
    } else {
        GameResult::Unknown
    }
}

/// Live Client Data API endpoint
const LIVE_CLIENT_API: &str = "https://127.0.0.1:2999/liveclientdata";

/// Basic game info from Live Client API (port 2999)
#[derive(Debug, Clone)]
pub struct LiveClientBasicInfo {
    pub summoner_name: String,
    pub champion_name: String,
    pub game_time: f32,
    /// Game mode string from API (e.g. "CLASSIC", "TFT", "ARAM")
    pub game_mode: String,
}

/// Shared HTTP client for the lightweight `check_live_client_basic` probe.
///
/// This function runs on the 1s game-monitor poll loop and the 2s frontend
/// `get_unified_game_status` poll, so building a fresh reqwest client (with a
/// full TLS context) on every call is wasteful. Reuse a single self-signed-cert
/// accepting client for the lifetime of the process.
static LIVE_CLIENT_HTTP: OnceLock<Client> = OnceLock::new();

fn live_client_http() -> Option<&'static Client> {
    if let Some(client) = LIVE_CLIENT_HTTP.get() {
        return Some(client);
    }
    match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(2000))
        .build()
    {
        Ok(client) => {
            // Ignore the error if another thread initialized it first; either way
            // `get()` below returns a valid client.
            let _ = LIVE_CLIENT_HTTP.set(client);
            LIVE_CLIENT_HTTP.get()
        }
        Err(e) => {
            tracing::debug!("Failed to build HTTP client: {}", e);
            None
        }
    }
}

/// Check Live Client API (port 2999) directly for game detection.
/// Returns `None` if the game is not running or still loading.
/// Timeout is fixed at 2000ms for reliability.
pub async fn check_live_client_basic() -> Option<LiveClientBasicInfo> {
    let client = live_client_http()?;

    let response = match client
        .get("https://127.0.0.1:2999/liveclientdata/allgamedata")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("Live Client API request failed: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::debug!("Live Client API returned status: {}", response.status());
        return None;
    }

    let json: serde_json::Value = match response.json().await {
        Ok(j) => j,
        Err(e) => {
            tracing::debug!("Failed to parse Live Client API response: {}", e);
            return None;
        }
    };

    let summoner_name = json["activePlayer"]["summonerName"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if summoner_name.is_empty() {
        tracing::debug!("Live Client API: summoner name is empty (game loading?)");
        return None;
    }

    let champion_name = json["allPlayers"]
        .as_array()
        .and_then(|players| {
            players
                .iter()
                .find(|p| p["summonerName"].as_str() == Some(&summoner_name))
        })
        .and_then(|player| player["championName"].as_str())
        .unwrap_or("Unknown")
        .to_string();

    let game_time = json["gameData"]["gameTime"].as_f64().unwrap_or(0.0) as f32;

    let game_mode = json["gameData"]["gameMode"]
        .as_str()
        .unwrap_or("CLASSIC")
        .to_string();

    tracing::debug!(
        "Live Client API: {} playing {} [{}] (time: {:.0}s)",
        summoner_name,
        champion_name,
        game_mode,
        game_time
    );

    Some(LiveClientBasicInfo {
        summoner_name,
        champion_name,
        game_time,
        game_mode,
    })
}

/// Event types that trigger automatic recording
#[derive(Debug, Clone, PartialEq)]
pub enum EventTrigger {
    ChampionKill,
    Death,      // Player death
    Assist,     // Player assist (without kill)
    FirstBlood, // First blood
    /// 내가 퍼블을 **당한** 것.
    ///
    /// Live Client API 의 `FirstBlood` 이벤트는 퍼블을 딴 쪽(`Recipient`)만 싣고
    /// 당한 쪽은 알려주지 않는다. 그래서 이건 그 이벤트가 아니라 **그 판의 첫
    /// `ChampionKill` 에서 내가 희생자인 경우**로 판정한다(`first_kill_seen`).
    FirstBloodVictim,
    Multikill(u8), // Double, Triple, Quadra, Penta
    DragonKill,
    BaronKill,
    HeraldKill, // Rift Herald
    TurretKill,
    InhibitorKill,
    Ace,
    Steal,   // Dragon/Baron steal
    GameEnd, // Game over
    ElderDragonKill,
    VoidgrubsKill,
    AtakhanKill,
    Shutdown,
    Outplay1vX(u32), // 1v2, 1v3, etc. - number is how many enemies involved
    TradeKill,       // Kill then die within 5s (trade kill / aggressive play)
    LowHpOutplay,    // Kill while below 20% HP
}

impl EventTrigger {
    /// Get clip priority (1-5)
    pub fn priority(&self) -> u8 {
        match self {
            EventTrigger::ChampionKill => 1,
            EventTrigger::Death => 1,
            EventTrigger::Assist => 1,
            EventTrigger::FirstBlood => 3,
            // 퍼블을 당한 것도 판을 가르는 순간이라 일반 데스(1)보다 높다.
            EventTrigger::FirstBloodVictim => 3,
            EventTrigger::Multikill(2) => 2, // Double
            EventTrigger::Multikill(3) => 3, // Triple
            EventTrigger::Multikill(4) => 4, // Quadra
            EventTrigger::Multikill(5) => 5, // Penta
            EventTrigger::DragonKill => 2,
            EventTrigger::BaronKill => 3,
            EventTrigger::HeraldKill => 2,
            EventTrigger::TurretKill => 1,
            EventTrigger::InhibitorKill => 2,
            EventTrigger::Ace => 4,
            EventTrigger::Steal => 4,
            EventTrigger::GameEnd => 3,
            EventTrigger::ElderDragonKill => 4,
            EventTrigger::VoidgrubsKill => 2,
            EventTrigger::AtakhanKill => 3,
            EventTrigger::Shutdown => 3,
            EventTrigger::Outplay1vX(n) => {
                if *n >= 3 {
                    5
                } else {
                    4
                }
            }
            EventTrigger::TradeKill => 2, // Trade kill (less impressive than former tower dive)
            EventTrigger::LowHpOutplay => 4,
            _ => 1,
        }
    }

    /// 이 트리거가 **승격의 결과**라면, 승격 전의 상위(부모) 트리거.
    ///
    /// # 왜 필요한가
    ///
    /// `detect_trigger` 는 게임 이벤트 하나당 트리거를 **하나만** 만든다. 그래서
    /// 킬 하나가 셧다운이거나 더블킬이면 그 순간은 더 이상 `ChampionKill` 이
    /// 아니다. 이 성질 때문에 "킬 켜기 + 셧다운 끄기" 조합에서 **셧다운 킬이
    /// 통째로 사라졌다** — 사용자는 킬을 담겠다고 했는데 가장 좋은 킬이 빠졌고,
    /// 화면은 아무 말도 하지 않았다. 기본 프리셋이 정확히 그 조합이었다.
    ///
    /// 부모를 알면 그 상태에서 클립을 버리는 대신 한 단계 내려 다시 판정할 수
    /// 있다(`resolve_recordable_trigger`). 그러면 설정 화면이 약속하는 "부모를
    /// 켜면 그 계열은 반드시 담긴다"가 실제로 참이 된다.
    ///
    /// 부모는 **더 일반적인 쪽**이다. 반대 방향(킬 → 셧다운)으로는 올라가지
    /// 않는다 — 승격은 감지가 하는 일이지 설정이 하는 일이 아니다.
    ///
    /// `event` 를 받는 이유는 스틸 때문이다. 스틸은 드래곤에서도 바론에서도
    /// 나오므로, 원본 이벤트를 보지 않으면 어느 쪽으로 내려야 할지 알 수 없다.
    pub fn parent(&self, event: &GameEvent) -> Option<EventTrigger> {
        match self {
            // `ChampionKill` 이벤트에서 더 특별한 이름을 얻은 것들.
            EventTrigger::Shutdown
            | EventTrigger::Multikill(_)
            | EventTrigger::Outplay1vX(_)
            | EventTrigger::LowHpOutplay => Some(EventTrigger::ChampionKill),
            // 둘 다 "내가 죽은" 이벤트다. 데스를 껐다면 이것들도 빠져야 한다 —
            // 예전에는 `record_trade_kill` 이 기본 on 이라 데스를 꺼 둔 사용자의
            // 클립에 죽는 장면이 섞여 들어갔다.
            EventTrigger::TradeKill | EventTrigger::FirstBloodVictim => Some(EventTrigger::Death),
            EventTrigger::ElderDragonKill => Some(EventTrigger::DragonKill),
            EventTrigger::Steal => match event.event_name.as_str() {
                "BaronKill" => Some(EventTrigger::BaronKill),
                // 장로를 스틸했으면 장로로 내려간다. 장로도 꺼져 있으면 그 다음
                // 단계에서 일반 드래곤까지 내려간다(강등은 반복된다).
                _ if event
                    .dragon_type
                    .as_deref()
                    .is_some_and(|t| t.contains("Elder")) =>
                {
                    Some(EventTrigger::ElderDragonKill)
                }
                _ => Some(EventTrigger::DragonKill),
            },
            _ => None,
        }
    }

    /// Get recommended clip duration before event (seconds)
    pub fn pre_duration(&self) -> u32 {
        match self {
            EventTrigger::Multikill(_)
            | EventTrigger::Outplay1vX(_)
            | EventTrigger::LowHpOutplay => 12,
            EventTrigger::Steal => 15,
            EventTrigger::Ace => 10,
            // 게임 종료: 예전에는 30초였다.
            //
            // 쇼츠 관점에서 이건 과했다 — 60초 영상의 2/3 를 한 장면이 먹었다. 게다가
            // 실측하면 확보되지도 않았다: 녹화가 게임과 함께 멈추므로 post 10초가
            // 존재할 수 없고, 실제 산출물은 40초가 아니라 31.4초였다.
            // 12+3 이면 승리 직전 한타와 넥서스 파괴가 들어가고, 남은 자리에 다른
            // 장면이 두 개 더 들어간다.
            EventTrigger::GameEnd => 12,
            EventTrigger::Death
            | EventTrigger::Assist
            | EventTrigger::FirstBloodVictim
            | EventTrigger::TradeKill
            | EventTrigger::TurretKill
            | EventTrigger::InhibitorKill => 6,
            EventTrigger::DragonKill
            | EventTrigger::BaronKill
            | EventTrigger::HeraldKill
            | EventTrigger::ElderDragonKill
            | EventTrigger::VoidgrubsKill
            | EventTrigger::AtakhanKill => 10,
            _ => 8,
        }
    }

    /// Get recommended clip duration after event (seconds)
    pub fn post_duration(&self) -> u32 {
        match self {
            EventTrigger::Ace => 10, // Show aftermath
            // 게임 종료의 post-roll 은 **원리적으로 확보되지 않는다** — 녹화가 게임과
            // 함께 멈추기 때문이다. 10초를 적어 두면 매번 부족분 경고만 남는다.
            EventTrigger::GameEnd => 3,
            EventTrigger::Steal => 10,
            EventTrigger::Multikill(_)
            | EventTrigger::Outplay1vX(_)
            | EventTrigger::LowHpOutplay => 8,
            EventTrigger::DragonKill
            | EventTrigger::BaronKill
            | EventTrigger::HeraldKill
            | EventTrigger::ElderDragonKill
            | EventTrigger::VoidgrubsKill
            | EventTrigger::AtakhanKill => 6,
            EventTrigger::Death
            | EventTrigger::Assist
            | EventTrigger::FirstBloodVictim
            | EventTrigger::TradeKill
            | EventTrigger::TurretKill
            | EventTrigger::InhibitorKill => 4,
            _ => 5,
        }
    }
}

/// Live Client API response structures
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AllGameData {
    #[serde(rename = "activePlayer", default)]
    pub active_player: ActivePlayer,
    #[serde(rename = "allPlayers", default)]
    pub all_players: Vec<Player>,
    #[serde(default)]
    pub events: Events,
    #[serde(rename = "gameData", default)]
    pub game_data: GameData,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ActivePlayer {
    #[serde(rename = "championName", default)]
    pub champion_name: String,
    #[serde(rename = "summonerName", default)]
    pub summoner_name: String,
    #[serde(default)]
    pub level: u32,
    #[serde(rename = "currentGold", default)]
    pub current_gold: f32,
    /// 내 체력 — **여기에만 있다.**
    ///
    /// `allPlayers[]` 에는 `championStats` 가 없다(라이엇 API 설계). 실게임 캡처로
    /// 확정했다: `activePlayer.championStats.maxHealth = 2458.76` 인데
    /// `allPlayers[0]` 에는 그 키 자체가 없다. 하이라이트 점수의 클러치 배수가
    /// 여기 걸려 있다 — `capture_moment` 참조.
    #[serde(rename = "championStats", default)]
    pub champion_stats: ChampionStats,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Player {
    #[serde(rename = "championName", default)]
    pub champion_name: String,
    #[serde(rename = "summonerName", default)]
    pub summoner_name: String,
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub scores: Scores,
    #[serde(rename = "isDead", default)]
    pub is_dead: bool,
    #[serde(rename = "championStats", default)]
    pub champion_stats: ChampionStats,
}

/// Champion stats from Live Client API (subset relevant for detection)
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ChampionStats {
    #[serde(rename = "currentHealth", default)]
    pub current_health: f32,
    #[serde(rename = "maxHealth", default)]
    pub max_health: f32,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Scores {
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(rename = "creepScore", default)]
    pub creep_score: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Events {
    #[serde(rename = "Events", default)]
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GameEvent {
    #[serde(rename = "EventID", default)]
    pub event_id: u32,
    #[serde(rename = "EventName", default)]
    pub event_name: String,
    #[serde(rename = "EventTime", default)]
    pub event_time: f32,
    /// The acting player for this event.
    ///
    /// Aliased because the Live Client API does not use `KillerName` uniformly:
    /// `FirstBlood` carries the credited player under `Recipient`, and `Ace`
    /// carries the player whose kill completed the ace under `Acer`. Reusing
    /// this field (instead of adding new ones) keeps `GameEvent`'s shape
    /// unchanged for the other construction sites in `auto_clip_manager.rs`
    /// that build it as a plain struct literal.
    #[serde(rename = "KillerName", default)]
    pub killer_name: Option<String>,
    /// `Acer` — Ace 이벤트에서 에이스를 완성한 플레이어.
    ///
    /// `killer_name` 에 alias 로 얹지 않는다: serde 는 같은 필드를 두 번 채우면
    /// `duplicate field` 로 **역직렬화 전체를 실패**시키고, 이벤트는 `Events` 배열로
    /// 한꺼번에 파싱되므로 이런 이벤트 하나가 그 배치의 모든 이벤트를 날린다.
    /// (실측: `{"KillerName":"A","Acer":"B"}` → `duplicate field \`KillerName\``)
    #[serde(rename = "Acer", default)]
    pub acer: Option<String>,
    /// `Recipient` — FirstBlood 이벤트에서 퍼블을 딴 플레이어. 위와 같은 이유로 별도 필드.
    #[serde(rename = "Recipient", default)]
    pub recipient: Option<String>,
    #[serde(rename = "VictimName", default)]
    pub victim_name: Option<String>,
    #[serde(rename = "Assisters", default)]
    pub assisters: Option<Vec<String>>,
    #[serde(rename = "DragonType", default)]
    pub dragon_type: Option<String>,
    /// `GameEnd` 이벤트가 실어 보내는 승패(`"Win"` / `"Lose"`).
    ///
    /// 이 값을 받기 전까지 `GameMetadata::result` 는 **모든 생성 지점에서
    /// `None`** 이었고(`auto_clip_manager` 3곳, `commands`, `game_lifecycle`),
    /// 유일한 쓰기는 "5분 미만이면 Remake" 라는 길이 추정뿐이었다. 즉 이긴 판과
    /// 진 판을 앱이 구분하지 못했다.
    ///
    /// 없을 수도 있다고 보고 `Option` 으로 받는다 — 이 필드가 안 오면 예전처럼
    /// 길이 추정으로 떨어질 뿐, 파싱이 깨지지는 않는다.
    #[serde(rename = "Result", default)]
    pub result: Option<String>,
    /// 이 이벤트가 일어난 **순간의 상황** — 하이라이트 점수의 배수가 전부 여기 걸린다.
    ///
    /// API 가 주는 값이 아니라 트리거를 감지한 직후 우리가 캐시에서 찍어 넣는다
    /// (`capture_moment`). 게임이 끝나면 Live Client API 는 사라지므로 나중에는
    /// 다시 구할 수 없고, 클립 저장 시점까지 이벤트에 실어 나르는 편이 콜백
    /// 시그니처를 바꾸는 것보다 파급이 작다.
    ///
    /// `skip` 인 이유: API 응답에는 없는 필드라 역직렬화 대상이 아니고, 이벤트가
    /// 프론트로 나갈 때도 점수는 클립 메타데이터 쪽에서 따로 나간다.
    #[serde(skip)]
    pub moment: Option<crate::recording::highlight_score::MomentContext>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GameData {
    #[serde(rename = "gameMode", default)]
    pub game_mode: String,
    #[serde(rename = "gameTime", default)]
    pub game_time: f32,
    #[serde(rename = "mapName", default)]
    pub map_name: String,
    #[serde(rename = "mapNumber", default)]
    pub map_number: u32,
}

/// Circuit breaker for resilient API polling.
///
/// Opens after `threshold` consecutive failures, then blocks requests until
/// the `cooldown` period expires (HALF-OPEN probe). A successful probe closes it.
struct CircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    cooldown: Duration,
    is_open: AtomicBool,
    last_failure: std::sync::Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            cooldown,
            is_open: AtomicBool::new(false),
            last_failure: std::sync::Mutex::new(None),
        }
    }

    /// Returns true if a request should be attempted (circuit CLOSED or HALF-OPEN).
    fn should_allow_request(&self) -> bool {
        if !self.is_open.load(Ordering::Relaxed) {
            return true;
        }
        // HALF-OPEN: allow one probe after cooldown expires
        if let Ok(last) = self.last_failure.lock() {
            if let Some(t) = *last {
                if t.elapsed() >= self.cooldown {
                    return true;
                }
            }
        }
        false
    }

    fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
            if let Ok(mut last) = self.last_failure.lock() {
                *last = Some(Instant::now());
            }
            tracing::warn!("Circuit breaker OPEN after {} consecutive failures", count);
        }
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        if self.is_open.swap(false, Ordering::Relaxed) {
            tracing::info!("Circuit breaker CLOSED after successful request");
        }
    }
}

/// Monitor for Live Client events with optimized event streaming
pub struct LiveClientMonitor {
    client: Client,
    config: EventStreamConfig,
    last_event_id: Arc<tokio::sync::Mutex<u32>>,
    player_name: Option<String>,
    recent_kills: Arc<tokio::sync::Mutex<Vec<KillRecord>>>,
    game_state_cache: Arc<RwLock<GameStateCache>>,
    last_full_data_fetch: Arc<tokio::sync::Mutex<Option<Instant>>>,
    kill_streak_tracker: Arc<tokio::sync::Mutex<HashMap<String, u32>>>,
    /// Recent champion kills near objectives for steal detection: (timestamp, killer_team)
    recent_champion_kills: Arc<tokio::sync::Mutex<Vec<(SystemTime, String)>>>,
    /// Circuit breaker to stop hammering the API after repeated failures
    circuit_breaker: CircuitBreaker,
    /// Recent solo kills by the player for 1vX outplay detection: (game_time, victim_name)
    recent_solo_kills: Arc<tokio::sync::Mutex<Vec<(f32, String)>>>,
    /// Recent player kills for tower dive detection: (game_time, victim_name)
    recent_player_kills_for_dive: Arc<tokio::sync::Mutex<Vec<(f32, String)>>>,
    /// 이 판에서 `ChampionKill` 을 하나라도 처리했는가.
    ///
    /// 퍼블을 **당한** 쪽을 알아내는 유일한 방법이다 — `FirstBlood` 이벤트는
    /// 딴 사람(`Recipient`)만 싣는다. 첫 킬의 희생자가 나면 그게 퍼블 데스다.
    first_kill_seen: Arc<tokio::sync::Mutex<bool>>,
    /// `GameEnd` 순간에 찍어 두는 내 전적. 게임이 끝나면 Live Client API 는 곧
    /// 사라지므로, 그 뒤에 조회해서는 챔피언도 KDA 도 알 수 없다. 세션을 마무리하는
    /// 쪽(`finish_auto_capture_session`)이 나중에 읽어 가도록 여기 남긴다.
    last_game_summary: Arc<RwLock<Option<PlayerSummary>>>,
}

/// `GameEnd` 시점에 찍은 내 전적 — 판 카드에 보여줄 값들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSummary {
    pub champion: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// `"Win"` / `"Lose"` 를 그대로. 해석은 저장 계층에서.
    pub result: Option<String>,
    /// `"CLASSIC"` / `"ARAM"` 등. 비어 있으면 못 얻은 것.
    ///
    /// 세션은 게임이 **로딩 중일 때** 시작되는 일이 흔한데, 그때는 Live Client
    /// API 가 아직 응답하지 않아 판 모드가 `UNKNOWN` 으로 굳었다(결과 화면에
    /// "트린다미어 - UNKNOWN" 으로 나갔다). 챔피언은 여기서 늦게 채워 주는 길이
    /// 이미 있었는데 모드만 없었다.
    pub game_mode: String,
}

#[derive(Debug, Clone)]
struct KillRecord {
    killer: String,
    timestamp: SystemTime,
}

/// Optimized game state cache to reduce redundant API calls
#[derive(Debug, Clone)]
struct GameStateCache {
    data: Option<AllGameData>,
    last_updated: Option<Instant>,
    ttl: Duration,
}

impl GameStateCache {
    fn new(ttl: Duration) -> Self {
        Self {
            data: None,
            last_updated: None,
            ttl,
        }
    }

    #[allow(dead_code)] // Future cache validation functionality
    fn is_valid(&self) -> bool {
        self.last_updated.is_some_and(|t| t.elapsed() < self.ttl)
    }

    fn update(&mut self, data: AllGameData) {
        self.data = Some(data);
        self.last_updated = Some(Instant::now());
    }

    #[allow(dead_code)] // Future cache access functionality
    fn get(&self) -> Option<&AllGameData> {
        if self.is_valid() {
            self.data.as_ref()
        } else {
            None
        }
    }
}

/// Event streaming configuration
#[derive(Debug, Clone, Copy)]
pub struct EventStreamConfig {
    /// Polling interval for event-only endpoint (faster)
    event_poll_interval: Duration,
    /// Polling interval for full game data (slower, fallback)
    full_data_interval: Duration,
    /// Cache TTL for game state
    #[allow(dead_code)] // Future cache tuning capability
    pub cache_ttl: Duration,
    /// Connection timeout
    connection_timeout: Duration,
    /// Task 30: Window (seconds) within which an enemy kill near an objective counts as contested.
    /// Wired to `check_contested_objective`. Defaults to `DEFAULT_CONTEST_WINDOW_SECS`.
    pub contest_window_secs: u64,
    /// Task 30: Window (seconds) within which champion kills are retained for steal detection.
    /// Wired to the kill-retention filter in `detect_trigger`. Defaults to `DEFAULT_KILL_RETENTION_SECS`.
    pub kill_retention_secs: u64,
}

impl Default for EventStreamConfig {
    fn default() -> Self {
        Self {
            event_poll_interval: Duration::from_millis(250), // 4x faster for events
            full_data_interval: Duration::from_secs(2),      // Slower for full data
            cache_ttl: Duration::from_millis(500),           // 500ms cache
            connection_timeout: Duration::from_secs(1),
            contest_window_secs: DEFAULT_CONTEST_WINDOW_SECS,
            kill_retention_secs: DEFAULT_KILL_RETENTION_SECS,
        }
    }
}

impl EventStreamConfig {
    /// Build a config from user-tunable event-filter settings, overriding only the
    /// steal-detection contest window. Polling intervals / timeouts keep their
    /// tuned defaults. (Task 30 wiring — previously `contest_window_secs` was
    /// defined in settings but never reached this config.)
    pub fn from_settings(contest_window_secs: u32) -> Self {
        Self {
            contest_window_secs: contest_window_secs as u64,
            ..Self::default()
        }
    }
}

/// Whether two Live Client player names refer to the same person.
///
/// The Live Client API is not consistent about Riot ID shape: `activePlayer
/// .summonerName` and `allPlayers[].summonerName` carry the tagged form
/// (`RIVEN1#KR1`), while the event feed's `KillerName` / `VictimName` /
/// `Assisters` have carried the bare game name (`RIVEN1`). Comparing them with
/// `==` silently dropped EVERY player-related event: in a field test with a
/// 4/4/13 scoreline, zero ChampionKill/Death/Assist triggers fired while `Ace`
/// — the one trigger that does no player matching — fired six times. The app
/// looked perfectly healthy and produced no highlight of the player at all.
///
/// Tag-insensitive matching is only applied when exactly one side carries a
/// `#TAG`. Two tagged names that differ only by tag are different people
/// (game names are not unique on their own), so those still compare unequal.
fn same_player(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let (a_name, a_tag) = split_riot_id(a);
    let (b_name, b_tag) = split_riot_id(b);
    if a_tag.is_some() != b_tag.is_some() {
        return a_name.eq_ignore_ascii_case(b_name);
    }
    false
}

/// Split `Name#TAG` into its parts. Returns `(name, None)` when untagged.
fn split_riot_id(value: &str) -> (&str, Option<&str>) {
    match value.split_once('#') {
        Some((name, tag)) => (name.trim(), Some(tag.trim())),
        None => (value, None),
    }
}

impl LiveClientMonitor {
    pub fn new() -> Result<Self> {
        Self::with_config(EventStreamConfig::default(), Arc::new(RwLock::new(None)))
    }

    /// 요약 슬롯을 **필수 인자**로 받는다.
    ///
    /// 처음에는 `share_summary_slot()` 이라는 선택적 setter 로 두었는데, 두 개의
    /// 모니터 생성 경로(수동: `auto_clip_manager`, 자동 감지: `game_monitor`) 중
    /// **한 곳에서만 부르는 바람에 주 동선인 자동 감지에서 요약이 통째로
    /// 버려졌다**. 컴파일은 통과했고 테스트 594개도 전부 green 이었다.
    ///
    /// 그래서 "부르는 걸 잊을 수 있는 API" 자체를 없앴다. 새 생성 경로가 생기면
    /// 슬롯을 어디서 가져올지 정하지 않고는 컴파일되지 않는다.
    pub fn with_config(
        config: EventStreamConfig,
        summary_slot: Arc<RwLock<Option<PlayerSummary>>>,
    ) -> Result<Self> {
        // Create HTTP client that accepts self-signed certificates
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(config.connection_timeout)
            .build()?;

        Ok(Self {
            client,
            config,
            last_event_id: Arc::new(tokio::sync::Mutex::new(0)),
            player_name: None,
            recent_kills: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            game_state_cache: Arc::new(RwLock::new(GameStateCache::new(config.cache_ttl))),
            last_full_data_fetch: Arc::new(tokio::sync::Mutex::new(None)),
            kill_streak_tracker: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            recent_champion_kills: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            // Open after 5 consecutive failures; reset after 30s cooldown
            circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(30)),
            recent_solo_kills: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            recent_player_kills_for_dive: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            first_kill_seen: Arc::new(tokio::sync::Mutex::new(false)),
            last_game_summary: summary_slot,
        })
    }

    /// Start optimized monitoring for events with intelligent polling
    pub async fn start_monitoring<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(EventTrigger, GameEvent) + Send + 'static,
    {
        info!("Starting optimized Live Client monitor...");

        // Reset last_event_id so a previous game's events don't carry over into this session.
        {
            *self.last_event_id.lock().await = 0;
            // 퍼블 데스 판정은 "이 판의 첫 킬"에 걸려 있다. 리셋하지 않으면 두 번째
            // 판부터는 첫 킬이 첫 킬로 보이지 않아 퍼블 데스가 영영 잡히지 않는다.
            *self.first_kill_seen.lock().await = false;
            info!("New monitoring session started");
        }

        let mut event_interval = time::interval(self.config.event_poll_interval);
        let mut full_data_interval = time::interval(self.config.full_data_interval);

        // Initialize player name with retry logic (game may still be loading)
        // League games can take 30-60+ seconds to fully load, especially on slower systems
        const MAX_INIT_RETRIES: u32 = 30;
        const INIT_RETRY_DELAY: Duration = Duration::from_secs(3);

        for attempt in 1..=MAX_INIT_RETRIES {
            match self.fetch_game_data().await {
                Ok(initial_data) => {
                    let player_name = initial_data.active_player.summoner_name.clone();
                    info!(
                        "✅ Connected to Live Client API - Monitoring player: {}",
                        player_name
                    );
                    self.player_name = Some(player_name);

                    // Skip past events: set last_event_id to the latest existing event
                    // so we only process NEW events from this point forward
                    if let Some(last_event) = initial_data.events.events.last() {
                        let mut last_id = self.last_event_id.lock().await;
                        *last_id = last_event.event_id;
                        info!(
                            "Skipping {} past events (last_event_id set to {})",
                            initial_data.events.events.len(),
                            last_event.event_id
                        );
                    }

                    self.game_state_cache.write().await.update(initial_data);
                    break;
                }
                Err(e) => {
                    if attempt < MAX_INIT_RETRIES {
                        info!(
                            "⏳ Waiting for game to load... (attempt {}/{}) - {}",
                            attempt, MAX_INIT_RETRIES, e
                        );
                        time::sleep(INIT_RETRY_DELAY).await;
                    } else {
                        tracing::warn!("Failed to fetch initial game data after {} attempts - will retry during polling", MAX_INIT_RETRIES);
                    }
                }
            }
        }

        // Task 31: Check for spectator mode before starting the event loop
        if self.is_spectating().await {
            info!("Spectator mode detected — skipping event processing for this session.");
            return Ok(());
        }

        info!(
            "🔄 Starting event monitoring loop (polling every {}ms, full refresh every {}ms)",
            self.config.event_poll_interval.as_millis(),
            self.config.full_data_interval.as_millis()
        );

        loop {
            tokio::select! {
                // High-frequency event polling (lightweight)
                _ = event_interval.tick() => {
                    if let Err(e) = self.poll_events_only(&mut on_event).await {
                        debug!("Event polling failed: {}", e);
                    }
                }

                // Low-frequency full data refresh (heavyweight)
                _ = full_data_interval.tick() => {
                    if let Err(e) = self.refresh_full_game_data(&mut on_event).await {
                        debug!("Full data refresh failed: {}", e);
                    }
                }
            }
        }
    }

    /// Lightweight event polling using eventdata endpoint
    async fn poll_events_only<F>(&mut self, on_event: &mut F) -> Result<()>
    where
        F: FnMut(EventTrigger, GameEvent) + Send + 'static,
    {
        if !self.circuit_breaker.should_allow_request() {
            debug!("Circuit breaker OPEN: skipping event poll");
            return Ok(());
        }

        match self.fetch_event_list().await {
            Ok(events) => {
                self.circuit_breaker.record_success();
                // Process only new events
                self.process_event_list(events, on_event).await?;
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                debug!("Event polling failed: {}", e);
            }
        }

        Ok(())
    }

    /// Full game data refresh (fallback and cache update)
    async fn refresh_full_game_data<F>(&mut self, on_event: &mut F) -> Result<()>
    where
        F: FnMut(EventTrigger, GameEvent) + Send + 'static,
    {
        if !self.circuit_breaker.should_allow_request() {
            debug!("Circuit breaker OPEN: skipping full data refresh");
            return Ok(());
        }

        match self.fetch_game_data().await {
            Ok(data) => {
                self.circuit_breaker.record_success();

                // Update cache
                {
                    let mut cache = self.game_state_cache.write().await;
                    cache.update(data.clone());
                }

                // Update last fetch time
                *self.last_full_data_fetch.lock().await = Some(Instant::now());

                // Update player name if needed (may have failed during initial connect)
                if self.player_name.is_none() {
                    self.player_name = Some(data.active_player.summoner_name.clone());
                    info!(
                        "✅ Live Client API connected - Monitoring player: {}",
                        data.active_player.summoner_name
                    );

                    // Late-connect guard: skip past events if init retries failed
                    // and this is the first successful data fetch during polling
                    if let Some(last_event) = data.events.events.last() {
                        let mut last_id = self.last_event_id.lock().await;
                        if *last_id == 0 {
                            *last_id = last_event.event_id;
                            info!(
                                "Late connect: skipping {} past events (last_event_id={})",
                                data.events.events.len(),
                                last_event.event_id
                            );
                        }
                    }
                }

                // Process events
                self.process_events(data, on_event).await?;
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                debug!("Full data refresh failed: {}", e);
            }
        }

        Ok(())
    }

    /// Fetch only the event list (lightweight endpoint)
    async fn fetch_event_list(&self) -> Result<Vec<GameEvent>> {
        let url = format!("{}/eventdata", LIVE_CLIENT_API);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send event list request")?;

        if response.status().is_success() {
            let wrapper: Events = response
                .json()
                .await
                .context("Failed to parse event data")?;
            Ok(wrapper.events)
        } else {
            Err(anyhow::anyhow!(
                "Event list endpoint returned status {}",
                response.status()
            ))
        }
    }

    /// Fetch current game data
    async fn fetch_game_data(&self) -> Result<AllGameData> {
        let url = format!("{}/allgamedata", LIVE_CLIENT_API);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Live Client API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "API returned status: {}",
                response.status()
            ));
        }

        // Get response text first for debugging
        let response_text = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Try to parse JSON
        let data: AllGameData = serde_json::from_str(&response_text).map_err(|e| {
            // Log the first 500 chars of response for debugging
            let preview = if response_text.len() > 500 {
                format!("{}...", &response_text[..500])
            } else {
                response_text.clone()
            };
            tracing::warn!("JSON parse error: {}. Response preview: {}", e, preview);
            anyhow::anyhow!("Failed to parse game data: {}", e)
        })?;

        // Validate that essential data is present (game may still be loading)
        if data.active_player.summoner_name.is_empty() {
            return Err(anyhow::anyhow!(
                "Game still loading: summoner name not available yet"
            ));
        }

        Ok(data)
    }

    /// Process event list (optimized for lightweight polling)
    async fn process_event_list<F>(&self, events: Vec<GameEvent>, on_event: &mut F) -> Result<()>
    where
        F: FnMut(EventTrigger, GameEvent) + Send + 'static,
    {
        let player_name = match &self.player_name {
            Some(name) => name.clone(),
            None => return Ok(()), // Skip if we don't know the player name yet
        };

        let mut game_ended = false;

        {
            let mut last_id = self.last_event_id.lock().await;

            for event in events {
                // Skip already processed events
                if event.event_id <= *last_id {
                    continue;
                }

                debug!("New event: {} at {}s", event.event_name, event.event_time);

                // Detect event triggers with cached player name
                if let Some(trigger) = self.detect_trigger(&event, &player_name).await {
                    info!(
                        "Event trigger detected: {:?} (priority: {})",
                        trigger,
                        trigger.priority()
                    );
                    if matches!(trigger, EventTrigger::GameEnd) {
                        game_ended = true;
                    }
                    // 상황은 지금 찍어야 한다 — 게임이 끝나면 Live Client API 가
                    // 사라져서 체력도 생존 인원도 되돌아가 물어볼 수 없다.
                    let mut enriched = event.clone();
                    enriched.moment = Some(self.capture_moment(&event, &player_name).await);
                    on_event(trigger, enriched);
                }

                *last_id = event.event_id;
            }
            // last_id guard dropped here
        }

        // Reset session state after GameEnd. Set last_event_id to MAX to prevent
        // the polling loop from re-processing all past events before it stops.
        if game_ended {
            *self.last_event_id.lock().await = u32::MAX;
            info!("GameEnd: setting last_event_id to MAX to block re-processing");
        }

        Ok(())
    }

    /// Process events and detect triggers (full data version)
    async fn process_events<F>(&self, data: AllGameData, on_event: &mut F) -> Result<()>
    where
        F: FnMut(EventTrigger, GameEvent),
    {
        let player_name = match self.player_name.as_ref() {
            Some(name) => name.clone(),
            None => {
                tracing::warn!("Player name not initialized - skipping event processing");
                return Ok(());
            }
        };

        let mut game_ended = false;

        {
            let mut last_id = self.last_event_id.lock().await;

            for event in &data.events.events {
                // Skip already processed events
                if event.event_id <= *last_id {
                    continue;
                }

                debug!("New event: {} at {}s", event.event_name, event.event_time);

                // Detect event triggers
                if let Some(trigger) = self.detect_trigger(event, &player_name).await {
                    info!(
                        "Event trigger detected: {:?} (priority: {})",
                        trigger,
                        trigger.priority()
                    );
                    if matches!(trigger, EventTrigger::GameEnd) {
                        game_ended = true;
                    }
                    let mut enriched = event.clone();
                    enriched.moment = Some(self.capture_moment(event, &player_name).await);
                    on_event(trigger, enriched);
                }

                *last_id = event.event_id;
            }
            // last_id guard dropped here
        }

        // Reset session state after GameEnd. Set last_event_id to MAX to prevent
        // the polling loop from re-processing all past events before it stops.
        if game_ended {
            *self.last_event_id.lock().await = u32::MAX;
            info!("GameEnd: setting last_event_id to MAX to block re-processing");
        }

        Ok(())
    }

    /// Detect if an event should trigger recording
    async fn detect_trigger(&self, event: &GameEvent, player_name: &str) -> Option<EventTrigger> {
        match event.event_name.as_str() {
            "FirstBlood" => {
                // FirstBlood credits the killer in `Recipient`; some payloads also
                // carry `KillerName`. Trigger only when that's us — the victim side is
                // already covered by the ChampionKill/Death branch below (gated by
                // `record_deaths`), so firing here too would double the clip up.
                if event
                    .recipient
                    .as_deref()
                    .or(event.killer_name.as_deref())
                    .is_some_and(|k| same_player(k, player_name))
                {
                    Some(EventTrigger::FirstBlood)
                } else {
                    None
                }
            }
            "ChampionKill" => {
                // 이 판의 첫 킬인가. `FirstBlood` 이벤트는 퍼블을 딴 쪽만 실어 보내므로,
                // 당한 쪽을 알 수 있는 곳은 여기뿐이다. 킬을 딴 경우든 당한 경우든
                // 첫 킬을 통과하면 표시가 서고, 판이 바뀌면 `start_monitoring` 이 지운다.
                let is_first_kill = {
                    let mut seen = self.first_kill_seen.lock().await;
                    let first = !*seen;
                    *seen = true;
                    first
                };

                // Track kill streaks for shutdown detection
                let mut streaks = self.kill_streak_tracker.lock().await;

                if let Some(killer) = &event.killer_name {
                    // Increment killer's streak
                    let killer_streak = streaks.entry(killer.clone()).or_insert(0);
                    *killer_streak += 1;
                }

                // Reset victim's streak and check for shutdown
                let victim_had_streak = if let Some(victim) = &event.victim_name {
                    let victim_streak = streaks.remove(victim).unwrap_or(0);
                    victim_streak >= 3
                } else {
                    false
                };

                drop(streaks);

                // Record champion kill with team for steal detection
                if let Some(killer) = &event.killer_name {
                    let cache = self.game_state_cache.read().await;
                    if let Some(ref data) = cache.data {
                        if let Some(killer_player) = data
                            .all_players
                            .iter()
                            .find(|p| same_player(&p.summoner_name, killer))
                        {
                            let mut recent = self.recent_champion_kills.lock().await;
                            let now = SystemTime::now();
                            recent.push((now, killer_player.team.clone()));
                            // Clean up old entries older than kill_retention_secs (Task 30)
                            let retention = self.config.kill_retention_secs;
                            recent.retain(|(ts, _)| {
                                now.duration_since(*ts).unwrap_or(Duration::from_secs(100))
                                    < Duration::from_secs(retention)
                            });
                        }
                    }
                }

                if let Some(killer) = &event.killer_name {
                    if same_player(killer, player_name) {
                        // Player got a kill - determine the best trigger

                        // Track this kill for 1vX and tower dive detection
                        let is_solo = event.assisters.as_ref().is_none_or(|a| a.is_empty());
                        {
                            let mut solo_kills = self.recent_solo_kills.lock().await;
                            if is_solo {
                                if let Some(victim) = &event.victim_name {
                                    solo_kills.push((event.event_time, victim.clone()));
                                }
                            }
                            // Prune kills older than 10s window
                            solo_kills.retain(|(t, _)| (event.event_time - t).abs() < 10.0);
                        }
                        {
                            let mut dive_kills = self.recent_player_kills_for_dive.lock().await;
                            if let Some(victim) = &event.victim_name {
                                dive_kills.push((event.event_time, victim.clone()));
                            }
                            // Prune kills older than 10s
                            dive_kills.retain(|(t, _)| (event.event_time - t).abs() < 10.0);
                        }

                        // Priority 1: Shutdown (killing a player on a streak)
                        if victim_had_streak {
                            return Some(EventTrigger::Shutdown);
                        }

                        // Priority 2: Check for multikill
                        let multikill = self.check_multikill(killer).await;

                        // Priority 3: Check for 1vX outplay
                        // Multiple solo kills within 10s window = 1vX
                        // 1vX takes priority over plain multikill when kills are solo
                        // Count unique victim names for the X in 1vX
                        let solo_kill_count = {
                            let solo_kills = self.recent_solo_kills.lock().await;
                            let unique_victims: std::collections::HashSet<&str> =
                                solo_kills.iter().map(|(_, name)| name.as_str()).collect();
                            unique_victims.len() as u32
                        };
                        if solo_kill_count >= 2 {
                            return Some(EventTrigger::Outplay1vX(solo_kill_count));
                        }

                        if multikill >= 2 {
                            return Some(EventTrigger::Multikill(multikill));
                        }

                        // Priority 4: Check for low HP outplay
                        // If player HP < 25% at kill time, it's a clutch play (25% to compensate for ~2s cache delay)
                        let low_hp = self.check_low_hp_outplay(player_name).await;
                        if low_hp {
                            return Some(EventTrigger::LowHpOutplay);
                        }

                        Some(EventTrigger::ChampionKill)
                    } else if event
                        .victim_name
                        .as_deref()
                        .is_some_and(|v| same_player(v, player_name))
                    {
                        // Player died - check for trade kill (player killed someone recently then died)
                        let dive_detected = {
                            let dive_kills = self.recent_player_kills_for_dive.lock().await;
                            // If player got a kill within 5s before dying, it's a trade kill
                            dive_kills
                                .iter()
                                .any(|(t, _)| (event.event_time - t).abs() < 5.0)
                        };
                        if dive_detected {
                            Some(EventTrigger::TradeKill)
                        } else if is_first_kill {
                            // 판의 첫 킬에서 내가 희생자 = 내가 퍼블을 당했다.
                            // (트레이드킬과는 겹칠 수 없다 — 먼저 킬을 냈다면 그쪽이
                            // 첫 킬이 되므로 여기 도달할 때는 이미 표시가 서 있다.)
                            Some(EventTrigger::FirstBloodVictim)
                        } else {
                            Some(EventTrigger::Death)
                        }
                    } else if let Some(assisters) = &event.assisters {
                        if assisters.iter().any(|a| same_player(a, player_name)) {
                            // Player got an assist
                            Some(EventTrigger::Assist)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "DragonKill" => {
                if event
                    .killer_name
                    .as_deref()
                    .is_some_and(|k| same_player(k, player_name))
                {
                    let is_contested = self.check_contested_objective(player_name).await;
                    if is_contested {
                        Some(EventTrigger::Steal)
                    } else if event
                        .dragon_type
                        .as_deref()
                        .map(|t| t.contains("Elder"))
                        .unwrap_or(false)
                    {
                        Some(EventTrigger::ElderDragonKill)
                    } else {
                        Some(EventTrigger::DragonKill)
                    }
                } else {
                    None
                }
            }
            "BaronKill" => {
                if event
                    .killer_name
                    .as_deref()
                    .is_some_and(|k| same_player(k, player_name))
                {
                    let is_contested = self.check_contested_objective(player_name).await;
                    if is_contested {
                        Some(EventTrigger::Steal)
                    } else {
                        Some(EventTrigger::BaronKill)
                    }
                } else {
                    None
                }
            }
            "TurretKilled" => {
                if let Some(killer) = &event.killer_name {
                    if same_player(killer, player_name) {
                        Some(EventTrigger::TurretKill)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "InhibKilled" => {
                if let Some(killer) = &event.killer_name {
                    if same_player(killer, player_name) {
                        Some(EventTrigger::InhibitorKill)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "HeraldKill" => {
                if event
                    .killer_name
                    .as_deref()
                    .is_some_and(|k| same_player(k, player_name))
                {
                    Some(EventTrigger::HeraldKill)
                } else {
                    None
                }
            }
            "HordeKill" => {
                if event
                    .killer_name
                    .as_deref()
                    .is_some_and(|k| same_player(k, player_name))
                {
                    Some(EventTrigger::VoidgrubsKill)
                } else {
                    None
                }
            }
            "AtakhanKill" => {
                if event
                    .killer_name
                    .as_deref()
                    .is_some_and(|k| same_player(k, player_name))
                {
                    Some(EventTrigger::AtakhanKill)
                } else {
                    None
                }
            }
            "Ace" => {
                // Field test finding: the previous code fired on every Ace event
                // unconditionally, so the enemy team's ace (i.e. the moment *we*
                // got wiped) was recorded as a priority-4 "highlight" alongside
                // our own. The Live Client API's Ace event has no documented,
                // stable field that names the acing team directly, so team
                // membership is inferred from state already tracked elsewhere:
                //
                // 1. Preferred: the API's `Acer` field — the player whose kill
                //    completed the ace. Look their team up
                //    in the cached `allPlayers` roster and compare to ours.
                // 2. Fallback: if the acer can't be resolved (name missing, or
                //    not present in the last roster snapshot), use the most
                //    recently recorded champion-kill's team from
                //    `recent_champion_kills` — that kill (the one that wiped
                //    the last enemy) fired its own ChampionKill event, and the
                //    branch above already recorded the killer's team.
                // 3. If neither signal resolves, do NOT trigger. An unrelated
                //    Ace being kept is worse than a real one being dropped.
                // 내 팀과 **내 생존 여부**를 함께 읽는다.
                //
                // 예전에는 팀만 확인했다. 그래서 내가 죽어 분수대에서 아이템을 사는
                // 동안 팀이 에이스하면 그 8초짜리 상점 화면이 "에이스 클립"으로
                // 저장됐다(실기기 확인 — 클립 전체에 액션이 0이었다).
                //
                // 쇼츠 관점에서 이런 클립은 **안 만들어지는 것보다 나쁘다**. 열어본
                // 사용자가 실망하고, 자동 편집이 이걸 골라 넣으면 영상 전체가 죽는다.
                let me = {
                    let cache = self.game_state_cache.read().await;
                    cache.data.as_ref().and_then(|data| {
                        data.all_players
                            .iter()
                            .find(|p| same_player(&p.summoner_name, player_name))
                            .map(|p| (p.team.clone(), p.is_dead))
                    })
                };
                let (my_team, i_was_dead) = match me {
                    Some(v) => v,
                    // Can't even tell which team the local player is on yet.
                    None => return None,
                };

                if i_was_dead {
                    debug!("Ace 무시: 내가 죽어 있는 동안 일어난 에이스라 볼 장면이 없다");
                    return None;
                }

                let mut acing_team = match event.killer_name.as_deref() {
                    Some(acer) => {
                        let cache = self.game_state_cache.read().await;
                        cache.data.as_ref().and_then(|data| {
                            data.all_players
                                .iter()
                                .find(|p| same_player(&p.summoner_name, acer))
                                .map(|p| p.team.clone())
                        })
                    }
                    None => None,
                };

                if acing_team.is_none() {
                    let recent = self.recent_champion_kills.lock().await;
                    acing_team = recent
                        .iter()
                        .max_by_key(|(ts, _)| *ts)
                        .map(|(_, team)| team.clone());
                }

                match acing_team {
                    Some(team) if team == my_team => Some(EventTrigger::Ace),
                    // Enemy team's ace, or undeterminable -> don't trigger.
                    _ => None,
                }
            }
            "GameEnd" => {
                // Task 29: Infer game result from duration at GameEnd
                let cache = self.game_state_cache.read().await;
                let game_time = cache
                    .data
                    .as_ref()
                    .map(|d| d.game_data.game_time as f64)
                    .unwrap_or(0.0);
                // 승패·챔피언·KDA 는 지금 찍어 두지 않으면 영영 못 얻는다 —
                // 게임이 끝나면 Live Client API 자체가 응답을 멈춘다.
                let summary = cache.data.as_ref().and_then(|data| {
                    data.all_players
                        .iter()
                        .find(|p| same_player(&p.summoner_name, player_name))
                        .map(|me| PlayerSummary {
                            champion: me.champion_name.clone(),
                            kills: me.scores.kills,
                            deaths: me.scores.deaths,
                            assists: me.scores.assists,
                            result: event.result.clone(),
                            game_mode: data.game_data.game_mode.clone(),
                        })
                });
                drop(cache);

                if let Some(ref s) = summary {
                    info!(
                        "GameEnd summary: {} {}/{}/{} result={:?}",
                        s.champion, s.kills, s.deaths, s.assists, s.result
                    );
                } else {
                    warn!("GameEnd: 내 전적을 찾지 못했습니다 (캐시 비었거나 이름 불일치)");
                }
                // `None` 으로 덮지 않는다. 이름 불일치나 캐시 공백으로 요약을
                // 못 만든 두 번째 GameEnd 가 앞서 제대로 찍힌 값을 지워서는 안 된다.
                if summary.is_some() {
                    *self.last_game_summary.write().await = summary;
                }

                let result = infer_game_result(game_time);
                info!(
                    "GameEnd detected: game_time={:.0}s, inferred_result={:?}",
                    game_time, result
                );
                // last_event_id reset is performed by the callers
                // (process_events / process_event_list) after this function returns
                // and the last_event_id lock is dropped, to avoid deadlock.
                Some(EventTrigger::GameEnd)
            }
            _ => None,
        }
    }

    /// Check if recent kills form a multikill
    async fn check_multikill(&self, killer: &str) -> u8 {
        let mut kills = self.recent_kills.lock().await;
        let now = SystemTime::now();

        // Add new kill
        kills.push(KillRecord {
            killer: killer.to_string(),
            timestamp: now,
        });

        // Remove old kills (>10 seconds)
        kills.retain(|k| {
            now.duration_since(k.timestamp)
                .unwrap_or(Duration::from_secs(100))
                < Duration::from_secs(10)
        });

        // Count kills by this player in the window
        let kill_count = kills.iter().filter(|k| k.killer == killer).count() as u8;

        // Return multikill level
        match kill_count {
            5.. => 5, // Pentakill
            4 => 4,   // Quadrakill
            3 => 3,   // Triple kill
            2 => 2,   // Double kill
            _ => 1,
        }
    }

    /// Check if an objective kill was contested (other team had kills within 10s)
    async fn check_contested_objective(&self, player_name: &str) -> bool {
        let cache = self.game_state_cache.read().await;
        let player_team = match cache.data.as_ref() {
            Some(data) => {
                match data
                    .all_players
                    .iter()
                    .find(|p| same_player(&p.summoner_name, player_name))
                {
                    Some(player) => player.team.clone(),
                    None => return false,
                }
            }
            None => return false,
        };
        drop(cache);

        let recent = self.recent_champion_kills.lock().await;
        let now = SystemTime::now();

        // Check if the OTHER team had kills within contest_window_secs seconds (Task 30)
        let contest_window = self.config.contest_window_secs;
        let contested = recent.iter().any(|(ts, team)| {
            let within_window = now.duration_since(*ts).unwrap_or(Duration::from_secs(100))
                < Duration::from_secs(contest_window);
            within_window && team != &player_team
        });

        if contested {
            info!("Contested objective detected: enemy team had recent kills near objective");
        }

        contested
    }

    /// Check if the player is below 25% HP (low HP outplay detection)
    ///
    /// Uses the cached game state to read the player's current health percentage.
    /// Returns true if player HP < 25% of max HP.
    /// Note: threshold is 25% instead of 20% to compensate for ~2s cache staleness
    /// in the Live Client API. The actual HP at the moment of the kill may be lower
    /// than the cached value we read here.
    async fn check_low_hp_outplay(&self, player_name: &str) -> bool {
        let cache = self.game_state_cache.read().await;
        if let Some(ref data) = cache.data {
            if let Some(player) = data
                .all_players
                .iter()
                .find(|p| same_player(&p.summoner_name, player_name))
            {
                let max_hp = player.champion_stats.max_health;
                let current_hp = player.champion_stats.current_health;
                if max_hp > 0.0 {
                    let hp_pct = current_hp / max_hp;
                    if hp_pct < 0.25 {
                        info!(
                            "Low HP outplay detected: {:.0}/{:.0} HP ({:.0}%)",
                            current_hp,
                            max_hp,
                            hp_pct * 100.0
                        );
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if Live Client API is available (optimized with cached connection)
    #[allow(dead_code)] // Future connection health checking
    pub async fn is_available(&self) -> bool {
        // Try eventdata endpoint first (lightweight)
        let url = format!("{}/eventdata", LIVE_CLIENT_API);

        match self.client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => {
                // Fallback to full endpoint
                let url = format!("{}/allgamedata", LIVE_CLIENT_API);
                matches!(self.client.get(&url).send().await, Ok(response) if response.status().is_success())
            }
        }
    }

    /// Detect if current game session is a replay (not a live game)
    ///
    /// Detection logic:
    /// 1. In a live game, the activePlayer matches one of the allPlayers
    /// 2. In a replay, the activePlayer summoner name is empty or doesn't match any participant
    /// 3. Additionally, we can check if the game mode or other indicators suggest replay
    ///
    /// Returns: Some(true) for replay, Some(false) for live game, None if detection fails
    pub async fn detect_replay_mode(&self) -> Option<bool> {
        // Fetch game data
        let data = match self.fetch_game_data().await {
            Ok(data) => data,
            Err(e) => {
                debug!("Failed to fetch game data for replay detection: {}", e);
                return None;
            }
        };

        // Get active player name
        let active_player_name = &data.active_player.summoner_name;

        // Check 1: Empty or placeholder active player name suggests replay mode
        if active_player_name.is_empty() || active_player_name == "Spectator" {
            info!("Replay detected: Active player name is empty or 'Spectator'");
            return Some(true);
        }

        // Check 2: Active player not in allPlayers list
        let is_participant = data
            .all_players
            .iter()
            .any(|p| same_player(&p.summoner_name, active_player_name));

        if !is_participant {
            info!(
                "Replay detected: Active player '{}' not found in game participants",
                active_player_name
            );
            return Some(true);
        }

        // Check 3: In spectator mode, activePlayer.level is 0
        if data.active_player.level == 0 {
            info!(
                "Spectator/replay detected: activePlayer.level == 0 for '{}'",
                active_player_name
            );
            return Some(true);
        }

        // Check 4: In replay mode, currentGold is usually 0 or static
        // (This is a heuristic - live games have fluctuating gold)
        // We'll use participant check as primary method

        info!(
            "Live game detected: Active player '{}' is a participant",
            active_player_name
        );
        Some(false)
    }

    /// Task 31: Returns `true` if the current session is a spectator/replay session.
    ///
    /// This is a convenience wrapper around `detect_replay_mode()` for call sites
    /// that only need a boolean answer.
    pub async fn is_spectating(&self) -> bool {
        self.detect_replay_mode().await.unwrap_or(false)
    }

    /// Get the current active player name (if known)
    pub fn get_active_player_name(&self) -> Option<&str> {
        self.player_name.as_deref()
    }

    /// Get all players in the current game
    /// 이벤트가 일어난 순간의 상황을 한 번에 찍는다.
    ///
    /// `highlight_score` 의 배수는 전부 이 값들에 걸려 있는데, 채우지 않으면
    /// 모든 배수가 1.0 이 되어 점수 모델이 예전(종류만 보는 5단계)과 똑같아진다.
    /// 즉 이 함수가 없으면 새 점수 모델은 껍데기다.
    ///
    /// 캐시를 **한 번만** 읽는다 — 신호마다 따로 읽으면 그 사이에 캐시가 갱신되어
    /// 서로 다른 순간의 값이 한 점수에 섞일 수 있다.
    pub async fn capture_moment(
        &self,
        event: &GameEvent,
        player_name: &str,
    ) -> crate::recording::highlight_score::MomentContext {
        use crate::recording::highlight_score::MomentContext;

        // 어시스트 수는 **이벤트 자체**가 알려준다 — 캐시와 무관하다.
        //
        // 예전에는 이 계산이 아래 캐시 가드 뒤에 있었다. 캐시가 비면 알 수 있는 값까지
        // 같이 버려져 단독 킬 판정(`Solo`, 배수 1.25)이 통째로 죽었다.
        // 내가 죽은 이벤트(Death)의 어시스트는 상대편 것이라 의미가 다르므로,
        // 킬러가 나일 때만 센다.
        let assist_count = match event.killer_name.as_deref() {
            Some(killer) if same_player(killer, player_name) => {
                Some(event.assisters.as_ref().map_or(0, |a| a.len() as u32))
            }
            _ => None,
        };

        let cache = self.game_state_cache.read().await;
        let Some(ref data) = cache.data else {
            // 캐시가 비어도 이벤트에서 아는 것은 남긴다.
            return MomentContext {
                assist_count,
                game_time_secs: Some(event.event_time as f64),
                ..Default::default()
            };
        };

        // 체력은 **`activePlayer`** 에서 읽는다.
        //
        // `allPlayers[]` 에는 `championStats` 가 **없다** — 라이엇 Live Client API 설계다
        // (실게임 캡처로 확정: `activePlayer.championStats.maxHealth = 2458.76`,
        // `allPlayers[0]` 에는 그 키 자체가 없음). 예전에는 없는 쪽에서 읽었고,
        // `#[serde(default)]` 때문에 파싱은 성공하고 체력만 조용히 0 이 됐다.
        // 그래서 `max > 0.0` 검사에 걸려 **항상 `None`** — "체력 8%에서 펜타킬" 이라는
        // 이 앱의 유일한 차별점이 한 번도 계산된 적이 없었다.
        let my_health_ratio = {
            let stats = &data.active_player.champion_stats;
            if stats.max_health > 0.0 {
                Some((stats.current_health / stats.max_health).clamp(0.0, 1.0) as f64)
            } else {
                // 최대 체력이 0 이면 아직 안 받은 값이다. 0 으로 나누지 않고 비운다.
                None
            }
        };

        // 양 팀 생존 수. 내 팀을 알아야 세므로 나를 못 찾으면 비운다.
        let me = data
            .all_players
            .iter()
            .find(|p| same_player(&p.summoner_name, player_name));

        let (allies_alive, enemies_alive) = match me {
            Some(me) => {
                let mut allies = 0u32;
                let mut enemies = 0u32;
                for p in &data.all_players {
                    if p.is_dead {
                        continue;
                    }
                    if p.team == me.team {
                        allies += 1;
                    } else {
                        enemies += 1;
                    }
                }
                (Some(allies), Some(enemies))
            }
            None => (None, None),
        };

        let game_time_secs = if data.game_data.game_time > 0.0 {
            Some(data.game_data.game_time as f64)
        } else {
            Some(event.event_time as f64)
        };

        MomentContext {
            my_health_ratio,
            assist_count,
            allies_alive,
            enemies_alive,
            game_time_secs,
            // 게임이 언제 끝날지는 그 순간에 알 수 없다. 마무리 단계에서 채운다.
            secs_before_game_end: None,
        }
    }

    pub async fn get_all_players(&self) -> Result<Vec<String>> {
        let data = self.fetch_game_data().await?;
        Ok(data
            .all_players
            .iter()
            .map(|p| p.summoner_name.clone())
            .collect())
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[test]
    fn test_starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        assert!(cb.should_allow_request());
    }

    #[test]
    fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.should_allow_request());
    }

    #[test]
    fn test_success_resets() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert!(cb.should_allow_request());
        // failure_count reset: needs 3 more failures to open again
        cb.record_failure();
        cb.record_failure();
        assert!(cb.should_allow_request());
        cb.record_failure();
        assert!(!cb.should_allow_request());
    }

    #[test]
    fn test_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(1));
        cb.record_failure();
        assert!(
            !cb.should_allow_request() || {
                // Depending on timing, may immediately be half-open
                std::thread::sleep(std::time::Duration::from_millis(5));
                cb.should_allow_request()
            }
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(cb.should_allow_request()); // HALF-OPEN after cooldown
    }
}

/// 실게임에서 받아 온 `allgamedata` 응답 하나. 소환사명만 익명화했고 **구조는 그대로**다.
///
/// 이 픽스처가 없던 동안 "체력을 어디서 읽어야 하는가" 가 추측으로만 논의됐고,
/// 그 결과 `allPlayers[].championStats`(존재하지 않는 필드)에서 읽는 코드가
/// 22개 클립을 만드는 동안 아무도 모르게 살아 있었다.
#[cfg(test)]
const REAL_GAME_DATA: &str = include_str!("../../tests/fixtures/live_client_allgamedata.json");

#[cfg(test)]
mod live_client_fixture_tests {
    use super::*;

    fn parsed() -> AllGameData {
        serde_json::from_str(REAL_GAME_DATA).expect("실게임 응답이 파싱돼야 한다")
    }

    /// 라이엇은 체력을 `activePlayer` 에만 준다 — 이 사실이 바뀌면 즉시 알아야 한다.
    #[test]
    fn health_lives_on_active_player_and_nowhere_else() {
        let data = parsed();

        assert!(
            data.active_player.champion_stats.max_health > 0.0,
            "activePlayer.championStats.maxHealth 가 비었다 — 체력 배수가 다시 죽는다"
        );

        // allPlayers 에는 championStats 가 아예 없어서 serde 기본값(0.0)으로 채워진다.
        // 여기서 읽으면 안 된다는 사실 자체를 고정한다.
        for player in &data.all_players {
            assert_eq!(
                player.champion_stats.max_health, 0.0,
                "allPlayers 에 championStats 가 생겼다면 이 테스트와 capture_moment 를 함께 갱신할 것"
            );
        }
    }

    /// 이벤트의 이름은 태그가 없고(`트린장로`) activePlayer 는 태그가 있다(`트린장로#0001`).
    /// `same_player` 가 그 차이를 흡수하지 못하면 단독 킬 판정이 통째로 죽는다.
    #[test]
    fn event_names_match_the_active_player_despite_the_tag() {
        let data = parsed();
        let me = &data.active_player.summoner_name;

        let my_kills = data
            .events
            .events
            .iter()
            .filter(|e| e.event_name == "ChampionKill")
            .filter(|e| e.killer_name.as_deref().is_some_and(|k| same_player(k, me)))
            .count();

        assert!(
            my_kills > 0,
            "실제로 킬을 딴 판인데 하나도 못 찾았다 — 이름 매칭이 깨졌다"
        );
    }

    /// 이 판에는 어시스트 0명짜리 단독 킬이 실제로 있었다.
    /// 그 킬들이 `Solo` 배수를 받아야 한다 — 지난 판에는 22개 전부 이유가 비어 있었다.
    #[test]
    fn the_real_game_contains_solo_kills_that_must_score_higher() {
        use crate::recording::highlight_score::{score, HighlightKind, MomentContext, ScoreReason};

        let data = parsed();
        let me = &data.active_player.summoner_name;

        let solo_kills = data
            .events
            .events
            .iter()
            .filter(|e| e.event_name == "ChampionKill")
            .filter(|e| e.killer_name.as_deref().is_some_and(|k| same_player(k, me)))
            .filter(|e| e.assisters.as_ref().is_none_or(|a| a.is_empty()))
            .count();

        assert!(
            solo_kills > 0,
            "단독 킬이 없는 픽스처로는 이 회귀를 지킬 수 없다"
        );

        // 단독 킬의 점수는 기본점보다 높아야 한다.
        let ctx = MomentContext {
            assist_count: Some(0),
            game_time_secs: Some(600.0),
            ..Default::default()
        };
        let scored = score(HighlightKind::Kill, &ctx);
        assert!(
            scored.value > HighlightKind::Kill.base(),
            "단독 킬인데 기본점 그대로다: {} (기본 {})",
            scored.value,
            HighlightKind::Kill.base()
        );
        assert!(
            scored.reasons.contains(&ScoreReason::Solo),
            "이유에 Solo 가 없다: {:?}",
            scored.reasons
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_trigger_priority() {
        assert_eq!(EventTrigger::ChampionKill.priority(), 1);
        assert_eq!(EventTrigger::Multikill(2).priority(), 2);
        assert_eq!(EventTrigger::Multikill(5).priority(), 5);
        assert_eq!(EventTrigger::BaronKill.priority(), 3);
        assert_eq!(EventTrigger::Ace.priority(), 4);
    }

    #[test]
    fn test_event_trigger_duration() {
        let trigger = EventTrigger::Multikill(3);
        assert_eq!(trigger.pre_duration(), 12);
        assert_eq!(trigger.post_duration(), 8);

        let trigger = EventTrigger::Steal;
        assert_eq!(trigger.pre_duration(), 15);
        assert_eq!(trigger.post_duration(), 10);
    }

    /// `Result` 필드의 **와이어 계약**을 고정한다.
    ///
    /// 이 파일에는 `Acer` 를 alias 로 얹었다가 `duplicate field` 로 **그 배치의
    /// 이벤트 전체를 날린** 전례가 있다. 이벤트는 배열로 한꺼번에 파싱되므로
    /// 필드 하나의 실수가 그 폴링의 모든 이벤트를 죽인다. 그래서 새 필드를 넣을
    /// 때는 실제 JSON 으로 왕복을 확인한다.
    #[test]
    fn game_end_result_parses_and_is_optional() {
        let with_result = r#"{"Events":[
            {"EventID":42,"EventName":"GameEnd","EventTime":1800.0,"Result":"Win"}
        ]}"#;
        let parsed: Events = serde_json::from_str(with_result).expect("Result 가 있어도 파싱된다");
        assert_eq!(parsed.events[0].result.as_deref(), Some("Win"));

        // Riot 공식 샘플 페이로드에는 GameEnd 이벤트 자체가 없다. 즉 이 필드가
        // 항상 온다는 보장이 없으므로, 없을 때도 반드시 성공해야 한다.
        let without_result = r#"{"Events":[
            {"EventID":42,"EventName":"GameEnd","EventTime":1800.0}
        ]}"#;
        let parsed: Events = serde_json::from_str(without_result).expect("없어도 파싱된다");
        assert_eq!(parsed.events[0].result, None);
    }

    /// 한 이벤트의 파싱 실패가 배치 전체를 죽이지 않는지는 이 파일의 다른
    /// 테스트가 다루지만, `Result` 가 문자열이 아닌 타입으로 오는 경우는 새 위험이다.
    #[test]
    fn a_non_string_result_does_not_take_down_the_whole_batch() {
        // 현재 구현은 `Option<String>` 이라 이 페이로드에서 배치가 통째로 실패한다.
        // 이 테스트는 그 사실을 **기록**한다 — 실제로 이런 페이로드가 관측되면
        // `serde_json::Value` 로 받아 `as_str()` 하는 관대한 파싱으로 바꿔야 한다.
        let odd =
            r#"{"Events":[{"EventID":1,"EventName":"GameEnd","EventTime":1.0,"Result":true}]}"#;
        let parsed: std::result::Result<Events, _> = serde_json::from_str(odd);
        assert!(
            parsed.is_err(),
            "지금은 실패한다. 이 단언이 깨지면 관대한 파싱이 들어왔다는 뜻이니 주석을 갱신할 것"
        );
    }

    #[tokio::test]
    async fn test_live_client_creation() {
        let monitor = LiveClientMonitor::new();
        assert!(monitor.is_ok());
    }

    // ---- GameResult inference tests ----

    #[test]
    fn test_infer_game_result_remake_under_300s() {
        assert_eq!(infer_game_result(0.0), GameResult::Remake);
        assert_eq!(infer_game_result(1.0), GameResult::Remake);
        assert_eq!(infer_game_result(299.9), GameResult::Remake);
    }

    #[test]
    fn test_infer_game_result_early_surrender_300_to_1200s() {
        assert_eq!(infer_game_result(300.0), GameResult::EarlySurrender);
        assert_eq!(infer_game_result(600.0), GameResult::EarlySurrender);
        assert_eq!(infer_game_result(1199.9), GameResult::EarlySurrender);
    }

    #[test]
    fn test_infer_game_result_unknown_over_1200s() {
        assert_eq!(infer_game_result(1200.0), GameResult::Unknown);
        assert_eq!(infer_game_result(2400.0), GameResult::Unknown);
        assert_eq!(infer_game_result(3600.0), GameResult::Unknown);
    }

    #[test]
    fn test_game_result_equality() {
        assert_eq!(GameResult::Remake, GameResult::Remake);
        assert_eq!(GameResult::Victory, GameResult::Victory);
        assert_ne!(GameResult::Remake, GameResult::Unknown);
    }

    // ---- EventTrigger priority tests for new variants ----

    #[test]
    fn test_event_trigger_priority_new_variants() {
        assert_eq!(EventTrigger::ElderDragonKill.priority(), 4);
        assert_eq!(EventTrigger::VoidgrubsKill.priority(), 2);
        assert_eq!(EventTrigger::AtakhanKill.priority(), 3);
        assert_eq!(EventTrigger::Shutdown.priority(), 3);
        assert_eq!(EventTrigger::Death.priority(), 1);
        assert_eq!(EventTrigger::Assist.priority(), 1);
        assert_eq!(EventTrigger::FirstBlood.priority(), 3);
        assert_eq!(EventTrigger::Steal.priority(), 4);
        assert_eq!(EventTrigger::GameEnd.priority(), 3);
        assert_eq!(EventTrigger::DragonKill.priority(), 2);
        assert_eq!(EventTrigger::HeraldKill.priority(), 2);
        assert_eq!(EventTrigger::TurretKill.priority(), 1);
        assert_eq!(EventTrigger::InhibitorKill.priority(), 2);
    }

    #[test]
    fn event_trigger_timing_table_matches_balanced_profile() {
        for trigger in [
            EventTrigger::ChampionKill,
            EventTrigger::FirstBlood,
            EventTrigger::Shutdown,
        ] {
            assert_eq!((trigger.pre_duration(), trigger.post_duration()), (8, 5));
        }
        for trigger in [
            EventTrigger::Death,
            EventTrigger::Assist,
            EventTrigger::TurretKill,
            EventTrigger::InhibitorKill,
        ] {
            assert_eq!((trigger.pre_duration(), trigger.post_duration()), (6, 4));
        }
        for trigger in [
            EventTrigger::Multikill(3),
            EventTrigger::Outplay1vX(2),
            EventTrigger::LowHpOutplay,
        ] {
            assert_eq!((trigger.pre_duration(), trigger.post_duration()), (12, 8));
        }
        for trigger in [
            EventTrigger::DragonKill,
            EventTrigger::BaronKill,
            EventTrigger::HeraldKill,
            EventTrigger::ElderDragonKill,
            EventTrigger::VoidgrubsKill,
            EventTrigger::AtakhanKill,
        ] {
            assert_eq!((trigger.pre_duration(), trigger.post_duration()), (10, 6));
        }
        assert_eq!(
            (
                EventTrigger::Steal.pre_duration(),
                EventTrigger::Steal.post_duration()
            ),
            (15, 10)
        );
        assert_eq!(
            (
                EventTrigger::Ace.pre_duration(),
                EventTrigger::Ace.post_duration()
            ),
            (10, 10)
        );
        assert_eq!(
            (
                EventTrigger::GameEnd.pre_duration(),
                EventTrigger::GameEnd.post_duration()
            ),
            (12, 3)
        );
    }

    #[test]
    fn test_event_trigger_pre_duration_new_variants() {
        assert_eq!(EventTrigger::ElderDragonKill.pre_duration(), 10);
        assert_eq!(EventTrigger::AtakhanKill.pre_duration(), 10);
        assert_eq!(EventTrigger::Shutdown.pre_duration(), 8);
        assert_eq!(EventTrigger::VoidgrubsKill.pre_duration(), 10);
        // 30 -> 12: 60초 쇼츠의 2/3 를 한 장면이 먹었다.
        assert_eq!(EventTrigger::GameEnd.pre_duration(), 12);
        assert_eq!(EventTrigger::Death.pre_duration(), 6);
        assert_eq!(EventTrigger::Assist.pre_duration(), 6);
        assert_eq!(EventTrigger::ChampionKill.pre_duration(), 8);
    }

    #[test]
    fn test_event_trigger_post_duration_new_variants() {
        assert_eq!(EventTrigger::ElderDragonKill.post_duration(), 6);
        assert_eq!(EventTrigger::AtakhanKill.post_duration(), 6);
        assert_eq!(EventTrigger::Shutdown.post_duration(), 5);
        assert_eq!(EventTrigger::VoidgrubsKill.post_duration(), 6);
        assert_eq!(EventTrigger::Ace.post_duration(), 10);
        // 10 -> 3: 녹화가 게임과 함께 멈추므로 post-roll 은 원리적으로 확보되지 않는다
        //          (실측: 40초 설계인데 산출물은 31.4초).
        assert_eq!(EventTrigger::GameEnd.post_duration(), 3);
        assert_eq!(EventTrigger::BaronKill.post_duration(), 6);
        assert_eq!(EventTrigger::ChampionKill.post_duration(), 5);
    }

    #[test]
    fn test_event_trigger_multikill_priority_all_levels() {
        // Double=2, Triple=3, Quadra=4, Penta=5
        assert_eq!(EventTrigger::Multikill(2).priority(), 2);
        assert_eq!(EventTrigger::Multikill(3).priority(), 3);
        assert_eq!(EventTrigger::Multikill(4).priority(), 4);
        assert_eq!(EventTrigger::Multikill(5).priority(), 5);
        // Values outside 2-5 fall through to default
        assert_eq!(EventTrigger::Multikill(1).priority(), 1);
    }

    // ---- Advanced event detection: priority tests ----

    #[test]
    fn test_outplay_1v2_priority() {
        assert_eq!(EventTrigger::Outplay1vX(2).priority(), 4);
    }

    #[test]
    fn test_outplay_1v3_or_more_priority() {
        assert_eq!(EventTrigger::Outplay1vX(3).priority(), 5);
        assert_eq!(EventTrigger::Outplay1vX(4).priority(), 5);
        assert_eq!(EventTrigger::Outplay1vX(5).priority(), 5);
    }

    #[test]
    fn test_trade_kill_priority() {
        assert_eq!(EventTrigger::TradeKill.priority(), 2);
    }

    #[test]
    fn test_low_hp_outplay_priority() {
        assert_eq!(EventTrigger::LowHpOutplay.priority(), 4);
    }

    // ---- Advanced event detection: duration tests ----

    #[test]
    fn test_outplay_1vx_durations() {
        let trigger = EventTrigger::Outplay1vX(2);
        assert_eq!(trigger.pre_duration(), 12);
        assert_eq!(trigger.post_duration(), 8);
    }

    #[test]
    fn test_trade_kill_durations() {
        let trigger = EventTrigger::TradeKill;
        assert_eq!(trigger.pre_duration(), 6);
        assert_eq!(trigger.post_duration(), 4);
    }

    #[test]
    fn test_low_hp_outplay_durations() {
        let trigger = EventTrigger::LowHpOutplay;
        assert_eq!(trigger.pre_duration(), 12);
        assert_eq!(trigger.post_duration(), 8);
    }

    // ---- Advanced event detection: detect_trigger integration tests ----

    /// Helper to create a LiveClientMonitor for testing
    fn create_test_monitor() -> LiveClientMonitor {
        LiveClientMonitor::with_config(EventStreamConfig::default(), Arc::new(RwLock::new(None)))
            .unwrap()
    }

    /// Helper to create a GameEvent for testing
    fn make_kill_event(
        event_id: u32,
        event_time: f32,
        killer: &str,
        victim: &str,
        assisters: Vec<String>,
    ) -> GameEvent {
        GameEvent {
            event_id,
            event_name: "ChampionKill".to_string(),
            event_time,
            killer_name: Some(killer.to_string()),
            victim_name: Some(victim.to_string()),
            assisters: Some(assisters),
            dragon_type: None,
            ..Default::default()
        }
    }

    /// Helper for events whose only relevant field is the actor bound into
    /// `killer_name` (FirstBlood's `Recipient`, Ace's `Acer` — see the alias
    /// on the struct field).
    fn make_named_event(
        event_id: u32,
        event_time: f32,
        event_name: &str,
        actor: Option<&str>,
    ) -> GameEvent {
        GameEvent {
            event_id,
            event_name: event_name.to_string(),
            event_time,
            killer_name: actor.map(|a| a.to_string()),
            victim_name: None,
            assisters: None,
            dragon_type: None,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_detect_low_hp_outplay_with_low_hp() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        // Set up game state cache with player at 15% HP
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    champion_name: "Yasuo".to_string(),
                    level: 10,
                    current_gold: 3000.0,
                    champion_stats: ChampionStats::default(),
                },
                all_players: vec![
                    Player {
                        summoner_name: player_name.to_string(),
                        champion_name: "Yasuo".to_string(),
                        team: "ORDER".to_string(),
                        level: 10,
                        scores: Scores::default(),
                        is_dead: false,
                        champion_stats: ChampionStats {
                            current_health: 150.0,
                            max_health: 1000.0, // 15% HP
                        },
                    },
                    Player {
                        summoner_name: "Enemy".to_string(),
                        champion_name: "Zed".to_string(),
                        team: "CHAOS".to_string(),
                        level: 10,
                        scores: Scores::default(),
                        is_dead: false,
                        champion_stats: ChampionStats::default(),
                    },
                ],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        let event = make_kill_event(1, 300.0, player_name, "Enemy", vec![]);
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::LowHpOutplay));
    }

    #[tokio::test]
    async fn test_detect_no_low_hp_outplay_with_high_hp() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        // Set up game state cache with player at 80% HP
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    champion_name: "Yasuo".to_string(),
                    level: 10,
                    current_gold: 3000.0,
                    champion_stats: ChampionStats::default(),
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    champion_name: "Yasuo".to_string(),
                    team: "ORDER".to_string(),
                    level: 10,
                    scores: Scores::default(),
                    is_dead: false,
                    champion_stats: ChampionStats {
                        current_health: 800.0,
                        max_health: 1000.0, // 80% HP
                    },
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        let event = make_kill_event(1, 300.0, player_name, "Enemy", vec![]);
        let trigger = monitor.detect_trigger(&event, player_name).await;
        // Should be a regular kill, not LowHpOutplay
        assert_eq!(trigger, Some(EventTrigger::ChampionKill));
    }

    #[tokio::test]
    async fn test_detect_1v2_outplay() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        // Set up game state with healthy player
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    team: "ORDER".to_string(),
                    champion_stats: ChampionStats {
                        current_health: 800.0,
                        max_health: 1000.0,
                    },
                    ..Default::default()
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // First solo kill at time 300
        let event1 = make_kill_event(1, 300.0, player_name, "Enemy1", vec![]);
        let trigger1 = monitor.detect_trigger(&event1, player_name).await;
        assert_eq!(trigger1, Some(EventTrigger::ChampionKill));

        // Second solo kill at time 305 (within 10s window) = 1v2
        let event2 = make_kill_event(2, 305.0, player_name, "Enemy2", vec![]);
        let trigger2 = monitor.detect_trigger(&event2, player_name).await;
        assert_eq!(trigger2, Some(EventTrigger::Outplay1vX(2)));
    }

    #[tokio::test]
    async fn test_detect_trade_kill_on_death() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        // Set up game state
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    team: "ORDER".to_string(),
                    champion_stats: ChampionStats {
                        current_health: 800.0,
                        max_health: 1000.0,
                    },
                    ..Default::default()
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // Player kills enemy at time 300
        let kill_event = make_kill_event(1, 300.0, player_name, "Enemy", vec![]);
        let _ = monitor.detect_trigger(&kill_event, player_name).await;

        // Player dies at time 303 (within 5s) = trade kill
        let death_event = make_kill_event(2, 303.0, "Enemy2", player_name, vec![]);
        let trigger = monitor.detect_trigger(&death_event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::TradeKill));
    }

    #[tokio::test]
    async fn test_no_trade_kill_if_death_too_late() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        // Set up game state
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    team: "ORDER".to_string(),
                    champion_stats: ChampionStats {
                        current_health: 800.0,
                        max_health: 1000.0,
                    },
                    ..Default::default()
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // Player kills enemy at time 300
        let kill_event = make_kill_event(1, 300.0, player_name, "Enemy", vec![]);
        let _ = monitor.detect_trigger(&kill_event, player_name).await;

        // Player dies at time 310 (>5s later) = regular death, not tower dive
        let death_event = make_kill_event(2, 310.0, "Enemy2", player_name, vec![]);
        let trigger = monitor.detect_trigger(&death_event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::Death));
    }

    // ---- FirstBlood: must be player-relevant, not unconditional ----

    #[tokio::test]
    async fn test_first_blood_triggers_when_player_is_recipient() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        let event = make_named_event(1, 90.0, "FirstBlood", Some(player_name));
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::FirstBlood));
    }

    #[tokio::test]
    async fn test_first_blood_none_when_another_player_is_recipient() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        let event = make_named_event(1, 90.0, "FirstBlood", Some("Enemy"));
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, None);
    }

    #[tokio::test]
    async fn test_first_blood_none_when_recipient_missing() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";

        let event = make_named_event(1, 90.0, "FirstBlood", None);
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, None);
    }

    // ---- Ace: must be OUR team's ace, not unconditional ----
    //
    // Field-test regression: a 4/4/13 scoreline (i.e. the player's team got
    // wiped) still produced an "Ace" highlight, because the old code fired on
    // every Ace event with no player/team relevance check at all.

    #[tokio::test]
    async fn test_ace_triggers_for_my_teams_ace_via_roster_lookup() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![
                    Player {
                        summoner_name: player_name.to_string(),
                        team: "ORDER".to_string(),
                        ..Default::default()
                    },
                    Player {
                        summoner_name: "Ally".to_string(),
                        team: "ORDER".to_string(),
                        ..Default::default()
                    },
                    Player {
                        summoner_name: "Enemy".to_string(),
                        team: "CHAOS".to_string(),
                        ..Default::default()
                    },
                ],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // Ally (same team) landed the ace-clinching kill.
        let event = make_named_event(1, 700.0, "Ace", Some("Ally"));
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::Ace));
    }

    #[tokio::test]
    async fn test_ace_none_for_enemy_teams_ace_via_roster_lookup() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![
                    Player {
                        summoner_name: player_name.to_string(),
                        team: "ORDER".to_string(),
                        ..Default::default()
                    },
                    Player {
                        summoner_name: "Enemy".to_string(),
                        team: "CHAOS".to_string(),
                        ..Default::default()
                    },
                ],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // Enemy team landed the ace-clinching kill -> this is the opposing
        // team's ace (we were the ones wiped). Must NOT trigger.
        let event = make_named_event(1, 700.0, "Ace", Some("Enemy"));
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, None);
    }

    #[tokio::test]
    async fn test_ace_falls_back_to_recent_kill_team_when_acer_not_in_roster() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    team: "ORDER".to_string(),
                    ..Default::default()
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // Player's own kill moments earlier records "ORDER" into
        // recent_champion_kills.
        let kill_event = make_kill_event(1, 699.0, player_name, "Enemy", vec![]);
        let _ = monitor.detect_trigger(&kill_event, player_name).await;

        // Ace event names an acer that isn't in the roster snapshot (e.g.
        // stale cache) -> falls back to the recent-kill-team heuristic.
        let ace_event = make_named_event(2, 700.0, "Ace", Some("UnknownAlly"));
        let trigger = monitor.detect_trigger(&ace_event, player_name).await;
        assert_eq!(trigger, Some(EventTrigger::Ace));
    }

    #[tokio::test]
    async fn test_ace_none_when_undeterminable() {
        let monitor = create_test_monitor();
        let player_name = "TestPlayer";
        {
            let mut cache = monitor.game_state_cache.write().await;
            cache.update(AllGameData {
                active_player: ActivePlayer {
                    summoner_name: player_name.to_string(),
                    ..Default::default()
                },
                all_players: vec![Player {
                    summoner_name: player_name.to_string(),
                    team: "ORDER".to_string(),
                    ..Default::default()
                }],
                events: Events::default(),
                game_data: GameData::default(),
            });
        }

        // No acer name and no recent champion kills recorded -> can't tell
        // which team was aced. Must default to NOT triggering.
        let event = make_named_event(1, 700.0, "Ace", None);
        let trigger = monitor.detect_trigger(&event, player_name).await;
        assert_eq!(trigger, None);
    }

    #[test]
    fn test_champion_stats_default() {
        let stats = ChampionStats::default();
        assert_eq!(stats.current_health, 0.0);
        assert_eq!(stats.max_health, 0.0);
    }
}

#[cfg(test)]
mod riot_id_matching_tests {
    use super::{same_player, split_riot_id};

    #[test]
    fn tagged_active_player_matches_untagged_event_name() {
        // The exact field-test case: activePlayer was "RIVEN1#KR1" while the event
        // feed reported "RIVEN1", and every kill/death/assist was discarded.
        assert!(same_player("RIVEN1#KR1", "RIVEN1"));
        assert!(same_player("RIVEN1", "RIVEN1#KR1"));
    }

    #[test]
    fn identical_names_match_in_either_shape() {
        assert!(same_player("RIVEN1", "RIVEN1"));
        assert!(same_player("RIVEN1#KR1", "RIVEN1#KR1"));
    }

    #[test]
    fn same_game_name_with_different_tags_are_different_people() {
        // Game names are not unique on their own; only the tag separates them.
        assert!(!same_player("RIVEN1#KR1", "RIVEN1#NA1"));
    }

    #[test]
    fn different_names_never_match() {
        assert!(!same_player("RIVEN1#KR1", "Faker"));
        assert!(!same_player("RIVEN1", "RIVEN2"));
        assert!(!same_player("RIVEN1", "RIVEN1X"));
    }

    #[test]
    fn empty_or_whitespace_never_matches() {
        assert!(!same_player("", "RIVEN1"));
        assert!(!same_player("RIVEN1", ""));
        assert!(!same_player("   ", "RIVEN1"));
    }

    #[test]
    fn comparison_is_case_and_whitespace_insensitive() {
        assert!(same_player(" riven1#KR1 ", "RIVEN1"));
    }

    #[test]
    fn korean_game_names_round_trip() {
        assert!(same_player("소환사#KR1", "소환사"));
        assert!(!same_player("소환사#KR1", "소환사2"));
    }

    #[test]
    fn split_riot_id_separates_name_and_tag() {
        assert_eq!(split_riot_id("RIVEN1#KR1"), ("RIVEN1", Some("KR1")));
        assert_eq!(split_riot_id("RIVEN1"), ("RIVEN1", None));
    }
}

#[cfg(test)]
mod event_payload_parsing_tests {
    use super::GameEvent;

    /// The whole Ace/FirstBlood fix rides on these JSON keys actually landing in the
    /// struct. Asserting on hand-built `GameEvent` values would prove nothing about
    /// that, so these go through serde exactly as the Live Client feed does.
    #[test]
    fn acer_and_recipient_deserialize_into_their_own_fields() {
        let ace: GameEvent =
            serde_json::from_str(r#"{"EventName":"Ace","Acer":"RIVEN1#KR1"}"#).unwrap();
        assert_eq!(ace.acer.as_deref(), Some("RIVEN1#KR1"));
        assert_eq!(ace.killer_name, None);

        let fb: GameEvent =
            serde_json::from_str(r#"{"EventName":"FirstBlood","Recipient":"RIVEN1"}"#).unwrap();
        assert_eq!(fb.recipient.as_deref(), Some("RIVEN1"));
    }

    /// Regression guard for a real trap: binding `Acer`/`Recipient` as serde *aliases* of
    /// `killer_name` made any payload carrying both keys fail with `duplicate field`, and
    /// because the feed is parsed as one `Events` array, a single such event would take
    /// every other event in the batch down with it — clip detection would go completely dead
    /// with nothing but a parse error in the log.
    #[test]
    fn an_event_carrying_both_keys_still_parses() {
        let json = r#"{"EventName":"Ace","KillerName":"A#KR1","Acer":"B#KR1"}"#;
        let parsed: GameEvent = serde_json::from_str(json).expect("both keys must coexist");
        assert_eq!(parsed.killer_name.as_deref(), Some("A#KR1"));
        assert_eq!(parsed.acer.as_deref(), Some("B#KR1"));
    }

    #[test]
    fn a_batch_survives_an_event_with_every_optional_key() {
        let json = r#"[
            {"EventID":1,"EventName":"ChampionKill","KillerName":"A","VictimName":"B"},
            {"EventID":2,"EventName":"Ace","Acer":"A","KillerName":"A"},
            {"EventID":3,"EventName":"FirstBlood","Recipient":"A"}
        ]"#;
        let events: Vec<GameEvent> = serde_json::from_str(json).expect("batch must parse");
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].acer.as_deref(), Some("A"));
        assert_eq!(events[2].recipient.as_deref(), Some("A"));
    }
}
