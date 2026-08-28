pub mod commands;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};
use thiserror::Error; // Ensure sysinfo is used

#[cfg(target_os = "windows")]
const LCU_PROCESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum LcuError {
    #[error("League client not found")]
    ClientNotFound,
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid lockfile format")]
    InvalidLockfile,
}

pub type Result<T> = std::result::Result<T, LcuError>;

/// Lockfile data parsed from League client lockfile
#[derive(Debug, Clone)]
pub struct LockfileData {
    /// Process name (for debugging purposes)
    #[allow(dead_code)]
    pub process_name: String,
    /// Process ID (for debugging purposes)
    #[allow(dead_code)]
    pub pid: u32,
    /// Port for LCU API connection (essential)
    pub port: u16,
    /// Password for LCU API authentication (essential)
    pub password: String,
    /// Protocol version (for debugging purposes)
    #[allow(dead_code)]
    pub protocol: String,
}

impl LockfileData {
    /// Parse lockfile format: ProcessName:PID:PORT:PASSWORD:PROTOCOL
    pub fn parse(content: &str) -> Result<Self> {
        let parts: Vec<&str> = content.trim().split(':').collect();

        if parts.len() != 5 {
            return Err(LcuError::InvalidLockfile);
        }

        Ok(Self {
            // Optional debug fields - stored for diagnostics
            process_name: parts[0].to_string(),
            pid: parts[1].parse().map_err(|_| LcuError::InvalidLockfile)?,
            port: parts[2].parse().map_err(|_| LcuError::InvalidLockfile)?,
            password: parts[3].to_string(),
            protocol: parts[4].to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub game_id: String,
    pub champion: String,
    pub game_mode: String,
    pub game_time: f64,
}

/// Game flow phase from LCU API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GameFlowPhase {
    None,
    Lobby,
    Matchmaking,
    CheckedIntoTournament,
    ReadyCheck,
    ChampSelect,
    GameStart,
    FailedToLaunch,
    InProgress,
    Reconnect,
    WaitingForStats,
    PreEndOfGame,
    EndOfGame,
    TerminatedInError,
}

/// Game session response from /lol-gameflow/v1/session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSession {
    pub phase: GameFlowPhase,
    #[serde(rename = "gameData")]
    pub game_data: Option<GameData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    #[serde(rename = "gameId")]
    pub game_id: i64,
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    #[serde(rename = "gameTime")]
    pub game_time: f64,
}

pub struct LcuClient {
    http_client: Option<reqwest::Client>,
    lockfile_data: Option<LockfileData>,
}

impl Default for LcuClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LcuClient {
    pub fn new() -> Self {
        Self {
            http_client: None,
            lockfile_data: None,
        }
    }

    /// Get the lockfile path by checking running processes first, then possible locations
    pub fn get_lockfile_path() -> Result<PathBuf> {
        tracing::info!("Attempting to detect League Client lockfile...");

        // Method 1: Dynamic Process Detection (sysinfo)
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All);

        for process in sys.processes().values() {
            let name = process.name().to_string_lossy().to_lowercase();
            if name == "leagueclientux.exe" || name == "leagueclientux" {
                if let Some(exe_path) = process.exe() {
                    if let Some(parent_dir) = exe_path.parent() {
                        let lockfile_path = parent_dir.join("lockfile");
                        if lockfile_path.exists() {
                            tracing::info!(
                                "Found lockfile via sysinfo: {}",
                                lockfile_path.display()
                            );
                            return Ok(lockfile_path);
                        }
                    }
                }
            }
        }

        // Method 2: Windows WMIC Command (Fallback if sysinfo fails due to permissions)
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            use std::process::Command;

            tracing::info!("sysinfo failed, trying WMIC...");

            // Execute: wmic process where "name='LeagueClientUx.exe'" get ExecutablePath
            let mut command = Command::new("wmic");
            command
                .args([
                    "process",
                    "where",
                    "name='LeagueClientUx.exe'",
                    "get",
                    "ExecutablePath",
                ])
                .creation_flags(0x08000000); // CREATE_NO_WINDOW
            let output = crate::utils::process::command_output_with_timeout(
                command,
                LCU_PROCESS_PROBE_TIMEOUT,
                "LCU WMIC process probe",
            );

            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.to_lowercase().contains("executablepath") {
                        continue;
                    }

                    let path = PathBuf::from(trimmed);
                    if let Some(parent) = path.parent() {
                        let lockfile_path = parent.join("lockfile");
                        if lockfile_path.exists() {
                            tracing::info!("Found lockfile via WMIC: {}", lockfile_path.display());
                            return Ok(lockfile_path);
                        }
                    }
                }
            }
        }

        // Method 3: Expanded Standard Paths (Brute Force)
        let mut possible_paths = Vec::new();

        // Common Drives
        let drives = vec!["C:", "D:", "E:", "F:", "G:"];

        for drive in drives {
            // Standard Riot Games folder
            possible_paths.push(PathBuf::from(format!(
                "{}\\{}",
                drive, "Riot Games\\League of Legends\\lockfile"
            )));
            // Program Files
            possible_paths.push(PathBuf::from(format!(
                "{}\\{}",
                drive, "Program Files\\Riot Games\\League of Legends\\lockfile"
            )));
        }

        // LocalAppData
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            possible_paths.push(
                PathBuf::from(local_app_data)
                    .join("Riot Games")
                    .join("League of Legends")
                    .join("lockfile"),
            );
        }

        // Try each path
        for path in possible_paths {
            if path.exists() {
                tracing::info!("Found lockfile at standard location: {}", path.display());
                return Ok(path);
            }
        }

        tracing::error!("Failed to find League Client lockfile. Ensure the client is running.");
        Err(LcuError::ClientNotFound)
    }

    /// Read and parse the lockfile with retries
    pub fn read_lockfile() -> Result<LockfileData> {
        let lockfile_path = Self::get_lockfile_path()?;

        let mut attempts = 0;
        let max_attempts = 5;

        loop {
            match fs::read_to_string(&lockfile_path) {
                Ok(content) => return LockfileData::parse(&content),
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(LcuError::Io(e));
                    }
                    // Wait a bit before retrying (file might be locked during writing)
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    /// Connect to the League client by reading lockfile
    pub async fn connect(&mut self) -> Result<()> {
        let lockfile = tokio::task::spawn_blocking(Self::read_lockfile)
            .await
            .map_err(|error| {
                LcuError::Connection(format!("LCU lockfile probe task failed: {error}"))
            })??;

        // Create HTTP client that accepts self-signed certificates
        let http_client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| LcuError::Connection(e.to_string()))?;

        self.lockfile_data = Some(lockfile);
        self.http_client = Some(http_client);

        if let Some(lockfile) = &self.lockfile_data {
            tracing::info!("Connected to LCU on port {}", lockfile.port);
        }

        Ok(())
    }

    /// Get the base URL for LCU API
    fn get_base_url(&self) -> Result<String> {
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        Ok(format!("https://127.0.0.1:{}", lockfile.port))
    }

    /// Get current game information
    pub async fn get_current_game(&self) -> Result<Option<GameInfo>> {
        let session = self.get_game_session().await?;

        match session.phase {
            GameFlowPhase::InProgress | GameFlowPhase::Reconnect => {
                if let Some(game_data) = session.game_data {
                    Ok(Some(GameInfo {
                        game_id: game_data.game_id.to_string(),
                        champion: "Unknown".to_string(), // Need to fetch from another endpoint
                        game_mode: game_data.game_mode,
                        game_time: game_data.game_time,
                    }))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Get game session from LCU API
    pub async fn get_game_session(&self) -> Result<GameSession> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        // Use gameflow-phase endpoint which is more stable and unlikely to 404
        let url = format!("{}/lol-gameflow/v1/gameflow-phase", base_url);

        let response = client
            .get(&url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LcuError::Api(format!("HTTP {}", response.status())));
        }

        let phase_str = response
            .text()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        // The response is a JSON string like "InProgress", need to trim quotes
        let phase_clean = phase_str.trim().trim_matches('"');

        let phase = match phase_clean {
            "InProgress" => GameFlowPhase::InProgress,
            "Reconnect" => GameFlowPhase::Reconnect,
            "ChampSelect" => GameFlowPhase::ChampSelect,
            "GameStart" => GameFlowPhase::GameStart,
            "Lobby" => GameFlowPhase::Lobby,
            "Matchmaking" => GameFlowPhase::Matchmaking,
            "WaitingForStats" => GameFlowPhase::WaitingForStats,
            "PreEndOfGame" => GameFlowPhase::PreEndOfGame,
            "EndOfGame" => GameFlowPhase::EndOfGame,
            "TerminatedInError" => GameFlowPhase::TerminatedInError,
            _ => GameFlowPhase::None,
        };

        // We don't get game_data from gameflow-phase, so set it to None
        // If we need game_data, we should call /lol-gameflow/v1/session separately if phase is InProgress
        // But for get_game_session struct, we need it.
        // Let's optimize: if InProgress, verify with session call.

        let mut game_data = None;

        if matches!(phase, GameFlowPhase::InProgress | GameFlowPhase::Reconnect) {
            let session_url = format!("{}/lol-gameflow/v1/session", base_url);
            if let Ok(resp) = client
                .get(&session_url)
                .basic_auth("riot", Some(&lockfile.password))
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(full_session) = resp.json::<serde_json::Value>().await {
                        // Parse game data manually or use struct
                        if let Some(data) = full_session.get("gameData") {
                            let id = data.get("gameId").and_then(|v| v.as_i64()).unwrap_or(0);
                            let mode = data
                                .get("gameMode")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown")
                                .to_string();
                            let time = data.get("gameTime").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            game_data = Some(GameData {
                                game_id: id,
                                game_mode: mode,
                                game_time: time,
                            });
                        }
                    }
                }
            }
        }

        Ok(GameSession { phase, game_data })
    }

    /// Check if a game is in progress
    pub async fn is_in_game(&self) -> Result<bool> {
        let session = self.get_game_session().await?;

        matches!(
            session.phase,
            GameFlowPhase::InProgress | GameFlowPhase::Reconnect
        )
        .then_some(true)
        .ok_or(LcuError::Api("Failed to check game state".to_string()))
        .or(Ok(false))
    }

    /// Check if client is connected
    pub fn is_connected(&self) -> bool {
        self.lockfile_data.is_some() && self.http_client.is_some()
    }

    /// Get performance metrics for monitoring
    pub async fn get_performance_metrics(&self) -> LcuMetrics {
        let is_connected = self.is_connected();
        let has_active_endpoint = is_connected && self.http_client.is_some();

        LcuMetrics {
            is_connected,
            cache_valid: is_connected, // Consider valid if connected
            cache_age_ms: if is_connected { 0 } else { u64::MAX },
            last_lockfile_check_age_ms: if self.lockfile_data.is_some() {
                0
            } else {
                u64::MAX
            },
            has_active_endpoint,
        }
    }

    /// Force refresh connection by re-reading lockfile
    pub async fn refresh_caches(&self) -> Result<()> {
        // LCU client is stateless except for http_client - reconnection is done via connect()
        // Check if we can still reach the API
        if self.is_connected() {
            if let Some(client) = &self.http_client {
                let base_url = self.get_base_url()?;
                let url = format!("{}/lol-summoner/v1/status", base_url);
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::debug!("LCU cache refresh: connection verified");
                        return Ok(());
                    }
                    _ => {
                        tracing::warn!("LCU cache refresh: connection lost, may need reconnect");
                    }
                }
            }
        }
        Ok(())
    }

    // ========================================================================
    // Match History & Replay Management
    // ========================================================================

    /// Get current summoner's match history
    pub async fn list_match_history(
        &self,
        begin_index: u32,
        end_index: u32,
    ) -> Result<Vec<MatchInfo>> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        // Get current summoner first
        let summoner_url = format!("{}/lol-summoner/v1/current-summoner", base_url);
        let summoner_resp = client
            .get(&summoner_url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !summoner_resp.status().is_success() {
            return Err(LcuError::Api("Failed to get current summoner".to_string()));
        }

        let summoner: serde_json::Value = summoner_resp
            .json()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;
        let puuid = summoner["puuid"]
            .as_str()
            .ok_or(LcuError::Api("Invalid summoner data".to_string()))?;

        // Get match history
        let history_url = format!(
            "{}/lol-match-history/v1/products/lol/{}/matches?begIndex={}&endIndex={}",
            base_url, puuid, begin_index, end_index
        );

        let history_resp = client
            .get(&history_url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !history_resp.status().is_success() {
            return Err(LcuError::Api("Failed to fetch match history".to_string()));
        }

        let history_data: serde_json::Value = history_resp
            .json()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;
        let games = history_data["games"]["games"]
            .as_array()
            .ok_or(LcuError::Api("Invalid match history format".to_string()))?;

        Ok(games
            .iter()
            .filter_map(|game| match_info_for_summoner(game, puuid))
            .collect())
    }

    /// Download replay for a specific game
    pub async fn download_replay(&self, game_id: i64) -> Result<()> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        let url = format!("{}/lol-replays/v1/rofls/{}/download", base_url, game_id);

        let resp = client
            .post(&url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(LcuError::Api(format!(
                "Failed to start replay download: {}",
                resp.status()
            )));
        }

        Ok(())
    }

    /// Get replay download status
    pub async fn get_replay_status(&self, game_id: i64) -> Result<String> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        let url = format!(
            "{}/lol-replays/v1/rofls/{}/download/status",
            base_url, game_id
        );

        let resp = client
            .get(&url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        // If 404, it means not downloaded
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok("NotDownloaded".to_string());
        }

        if !resp.status().is_success() {
            return Err(LcuError::Api("Failed to check replay status".to_string()));
        }

        // Returns raw string like "downloading" or "complete"?
        // API returns simple JSON usually. Let's treat as string for now.
        let status = resp
            .text()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;
        Ok(status.trim_matches('"').to_string())
    }

    /// Launch replay in League Client
    pub async fn launch_replay(&self, game_id: i64) -> Result<()> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        let url = format!("{}/lol-replays/v1/rofls/{}/watch", base_url, game_id);

        let resp = client
            .post(&url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(LcuError::Api("Failed to launch replay".to_string()));
        }

        Ok(())
    }

    /// Get game participants (for Target Selection in Replay)
    pub async fn get_game_participants(&self) -> Result<Vec<PlayerInfo>> {
        let client = self
            .http_client
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;
        let lockfile = self
            .lockfile_data
            .as_ref()
            .ok_or(LcuError::Connection("Not connected".to_string()))?;

        let base_url = self.get_base_url()?;
        let url = format!("{}/lol-gameflow/v1/session", base_url);

        let resp = client
            .get(&url)
            .basic_auth("riot", Some(&lockfile.password))
            .send()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(LcuError::Api("Failed to get game session".to_string()));
        }

        let session: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LcuError::Api(e.to_string()))?;
        let team_one = session["gameData"]["teamOne"]
            .as_array()
            .ok_or(LcuError::Api("Invalid team data".to_string()))?;
        let team_two = session["gameData"]["teamTwo"]
            .as_array()
            .ok_or(LcuError::Api("Invalid team data".to_string()))?;

        let mut players = Vec::with_capacity(team_one.len() + team_two.len());
        players.extend(
            team_one
                .iter()
                .enumerate()
                .map(|(index, player)| replay_player_info(player, 100, index)),
        );
        players.extend(
            team_two
                .iter()
                .enumerate()
                .map(|(index, player)| replay_player_info(player, 200, index)),
        );

        Ok(players)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    pub game_id: i64,
    pub game_mode: String,
    pub game_creation: i64,
    pub game_duration: i64,
    pub champion_id: u32,
    pub win: bool,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
}

/// Extract the queried summoner from an LCU match-history item. Match history
/// commonly returns all ten participants; participant 0 is not guaranteed to
/// be the current account. Prefer a direct PUUID, then the legacy
/// participantIdentities -> participantId join, and only accept a positional
/// fallback when the endpoint returned exactly one participant.
fn match_info_for_summoner(game: &serde_json::Value, puuid: &str) -> Option<MatchInfo> {
    let game_id = game.get("gameId")?.as_i64()?;
    if game_id <= 0 {
        return None;
    }
    let participants = game.get("participants")?.as_array()?;
    let participant = participants
        .iter()
        .find(|participant| {
            participant.get("puuid").and_then(serde_json::Value::as_str) == Some(puuid)
        })
        .or_else(|| {
            let participant_id = game
                .get("participantIdentities")?
                .as_array()?
                .iter()
                .find(|identity| {
                    identity
                        .get("player")
                        .and_then(|player| player.get("puuid"))
                        .and_then(serde_json::Value::as_str)
                        == Some(puuid)
                })?
                .get("participantId")?
                .as_i64()?;
            participants.iter().find(|participant| {
                participant
                    .get("participantId")
                    .and_then(serde_json::Value::as_i64)
                    == Some(participant_id)
            })
        })
        .or_else(|| (participants.len() == 1).then(|| &participants[0]))?;
    let stats = participant.get("stats")?;

    Some(MatchInfo {
        game_id,
        game_mode: game
            .get("gameMode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Unknown")
            .to_string(),
        game_creation: game
            .get("gameCreation")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        game_duration: game
            .get("gameDuration")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        champion_id: participant
            .get("championId")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
        win: stats
            .get("win")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        kills: match_stat_u32(stats, "kills"),
        deaths: match_stat_u32(stats, "deaths"),
        assists: match_stat_u32(stats, "assists"),
    })
}

fn match_stat_u32(stats: &serde_json::Value, key: &str) -> u32 {
    stats
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub summoner_name: String,
    pub champion_id: u32,
    pub team_id: u32,
}

fn replay_player_info(player: &serde_json::Value, team_id: u32, index: usize) -> PlayerInfo {
    let summoner_name = ["riotIdGameName", "gameName", "summonerName"]
        .into_iter()
        .find_map(|key| {
            player
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("Player {}", index + 1));
    let champion_id = player
        .get("championId")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    PlayerInfo {
        summoner_name,
        champion_id,
        team_id,
    }
}

/// Performance metrics for monitoring LCU client health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcuMetrics {
    pub is_connected: bool,
    pub cache_valid: bool,
    pub cache_age_ms: u64,
    pub last_lockfile_check_age_ms: u64,
    pub has_active_endpoint: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcu_client_creation() {
        let client = LcuClient::new();
        assert!(!client.is_connected());
    }

    #[test]
    fn test_lockfile_parse_valid() {
        let content = "LeagueClient:12345:54321:secret123:https";
        let lockfile = LockfileData::parse(content).unwrap();

        assert_eq!(lockfile.process_name, "LeagueClient");
        assert_eq!(lockfile.pid, 12345);
        assert_eq!(lockfile.port, 54321);
        assert_eq!(lockfile.password, "secret123");
        assert_eq!(lockfile.protocol, "https");
    }

    #[test]
    fn match_history_joins_the_queried_puuid_instead_of_using_participant_zero() {
        let game = serde_json::json!({
            "gameId": 42,
            "gameMode": "CLASSIC",
            "gameCreation": 1000,
            "gameDuration": 1800,
            "participantIdentities": [
                { "participantId": 1, "player": { "puuid": "opponent" } },
                { "participantId": 7, "player": { "puuid": "current-player" } }
            ],
            "participants": [
                {
                    "participantId": 1,
                    "championId": 1,
                    "stats": { "win": false, "kills": 1, "deaths": 9, "assists": 2 }
                },
                {
                    "participantId": 7,
                    "championId": 99,
                    "stats": { "win": true, "kills": 12, "deaths": 2, "assists": 8 }
                }
            ]
        });

        let parsed = match_info_for_summoner(&game, "current-player").unwrap();
        assert_eq!(parsed.game_id, 42);
        assert_eq!(parsed.champion_id, 99);
        assert!(parsed.win);
        assert_eq!((parsed.kills, parsed.deaths, parsed.assists), (12, 2, 8));
    }

    #[test]
    fn match_history_does_not_guess_when_a_multi_participant_identity_is_missing() {
        let game = serde_json::json!({
            "gameId": 42,
            "participants": [
                { "participantId": 1, "stats": {} },
                { "participantId": 2, "stats": {} }
            ]
        });

        assert!(match_info_for_summoner(&game, "missing").is_none());
    }

    #[test]
    fn replay_player_uses_riot_id_and_explicit_team() {
        let player = serde_json::json!({
            "riotIdGameName": "Current Name",
            "summonerName": "Legacy Name",
            "championId": 22
        });

        let parsed = replay_player_info(&player, 200, 0);
        assert_eq!(parsed.summoner_name, "Current Name");
        assert_eq!(parsed.champion_id, 22);
        assert_eq!(parsed.team_id, 200);
    }
}
