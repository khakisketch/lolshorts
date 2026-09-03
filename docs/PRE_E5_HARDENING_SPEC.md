# Pre-E5 Hardening Spec

> 출처: 2026-09-03 병렬 감사 (HEAD `3c84812`). 원장 = `.ksi/` (`ksi-goals.py --dir . status`).
> 마스터 플랜 = [PROJECT_IMPROVEMENT_MASTER_BACKLOG.md](./PROJECT_IMPROVEMENT_MASTER_BACKLOG.md) §5(P1). 이 문서는 그 P1에
> "실게임 전에 없앨 사일런트 버그"를 구체 스펙으로 편입한 것.
>
> **작업 분담:** Claude = 스펙·리뷰·통합·게이트. codex = 구현. codex는 자기 worktree에서
> 아래 항목을 브랜치 단위로 구현하고, Claude가 항목별로 adversarial 리뷰 후 `main`에 통합한다.
>
> **codex 작업 규칙:**
> - `main`(`3c84812`)에서 분기. 한 항목 = 한 커밋(또는 소수 커밋), 커밋 메시지에 `G00x` 참조.
> - 완료 게이트: `npm run typecheck && npm run lint && npm run build && npm run test:unit`
>   + `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
>   (PowerShell, `$env:PKG_CONFIG_PATH='C:\gstreamer-pkgconfig'`). **파이프로 종료코드 삼키지 말 것.**
> - 각 수정은 **outcome 기반 회귀 테스트**로 고정 — "값이 전달된다"가 아니라 "그 값이 실제로 트리거/점수를 움직인다".
> - `git commit`/`stash`/`checkout`으로 다른 worktree 상태를 건드리지 말 것.
> - 캡처·랭킹 알고리즘을 감으로 조정하지 말 것 (마스터 백로그 §11). 이 스펙은 **죽은 경로를 살리는 것**이지 튜닝이 아니다.

---

## P0 — 실게임 전 필수 (조용히 그 판을 통째로 날리는 버그)

이 클래스는 codex가 `convert_live_event`/`capture_moment`에서 이미 한 번 고쳤다
(`allPlayers[]`엔 없는 필드에서 읽어 조용히 0). **같은 클래스가 3곳 더 남아 있다.**

### G001 — `check_low_hp_outplay`가 죽은 필드에서 체력을 읽는다 (HIGH, 확인됨)

- **파일:** `src-tauri/src/recording/live_client.rs:1595-1620`
- **증상:** `data.all_players.iter().find(...).champion_stats.max_health` — Riot Live Client API는
  `championStats`를 `activePlayer`에만 준다. `allPlayers[]`엔 그 키가 없어 serde 기본값 `0.0`.
  → `if max_hp > 0.0` 항상 거짓 → **`LowHpOutplay` 트리거가 프로덕션에서 한 번도 발화한 적 없다.**
  같은 파일 테스트 `health_lives_on_active_player_and_nowhere_else`(1889)가 `allPlayers`의
  `max_health == 0.0`을 명시적으로 단언 중.
- **왜 중요:** "체력 8%에서 펜타킬"이 이 앱의 유일한 방어 가능한 차별점. 그 **감지**가 죽어 있다.
  `capture_moment`(1760)는 이미 `data.active_player.champion_stats`로 고쳐졌는데 이 함수는 누락됨.
- **수정 방향:** `check_low_hp_outplay(player_name)`의 `player_name`은 항상 로컬 플레이어이므로
  `data.active_player.champion_stats`에서 읽는다. (안전장치: `same_player(&data.active_player.summoner_name, player_name)`로
  일치 확인 후 읽고, 불일치면 `false`.)
- **합격기준:** ① active_player에서 읽음 ② 실 피드 픽스처(`activePlayer.championStats`만 있고
  `allPlayers`엔 없음)로 트리거 발화 테스트 ③ E5 로그에 `Low HP outplay detected` 출현.

### G002 — `fetch_event_list` 전체 배치 파싱: 이벤트 하나가 깨지면 그 판 클립 0개 (HIGH, 확인됨)

- **파일:** `src-tauri/src/recording/live_client.rs:952-974` (`fetch_event_list`), 구조체 `Events`/`GameEvent` `420-496`
- **증상:** `/eventdata` 피드는 **누적**(매 폴링마다 게임 시작부터 전체 반환). `response.json::<Events>()`는
  배열 전체를 한 번에 역직렬화 — 이벤트 하나라도 필드 **타입**이 예상과 다르면(`"EventTime":"123"`,
  `"Result":true` 등) 전체 실패. `#[serde(default)]`는 **누락**만 막지 타입 불일치는 못 막는다.
  피드가 누적이라 같은 나쁜 이벤트가 매번 와서 circuit breaker가 열리고 30초 재시도로 떨어짐 →
  그 판 이벤트 클립 0개, `debug!`로만 로그. 테스트 `a_non_string_result_does_not_take_down_the_whole_batch`(2021)가
  현재 `parsed.is_err()`를 단언하며 "관대한 파싱으로 바꿔야 한다"고 명시.
- **왜 중요:** Riot는 신규 이벤트 타입을 계속 추가(Atakhan, Voidgrubs 변형 등). 예상 밖 shape 하나 =
  그 판 전체 손실, 조용히. E5에서 신규 이벤트가 나오면 바로 발현.
- **수정 방향:** `Events`를 `{ #[serde(rename="Events")] events: Vec<serde_json::Value> }`로 받고,
  각 `Value`를 개별적으로 `serde_json::from_value::<GameEvent>()` 시도 → 실패한 것만 `warn!`(EventName/EventID
  프리뷰 포함) 후 스킵, 성공한 것만 반환. 알 수 없는 `EventName`은 정상 파싱되면 그대로 흘려보냄
  (`detect_trigger`가 이미 미지 이벤트를 무시).
- **합격기준:** ① per-event 관대한 파싱 ② 깨진 이벤트 1개 + 정상 이벤트 3개 → 정상 3개 처리됨을 테스트
  ③ `a_non_string_result_does_not_take_down_the_whole_batch`를 "이제 통과(정상 이벤트는 살아남음)"로 갱신
  ④ 스킵 시 `warn!` 로그 (조용하지 않게).

### G003 — `Ace` 트리거가 `killer_name`을 읽는다, 실제 필드는 `acer` (HIGH, 확인됨)

- **파일:** `src-tauri/src/recording/live_client.rs:1441` (`match event.killer_name.as_deref()`)
- **증상:** `Ace` 이벤트의 행위자는 `Acer` JSON 키 → `event.acer` 필드로 역직렬화(struct 주석 445-455,
  `killer_name`에 alias 안 얹음 — duplicate field 방지). `detect_trigger`의 `"Ace"` 갈래는
  `event.killer_name`을 읽음 → 실 payload에서 항상 `None` → "선호 경로"가 죽고
  `recent_champion_kills` 최신 킬 팀 폴백만 동작. 마지막 킬이 적팀이면 **우리 팀 에이스가 버려지거나
  적 에이스가 클립으로 남는다.** 테스트가 통과하는 건 `make_named_event`(2263)가 actor를
  `killer_name`에 넣기 때문 — 실 피드 shape와 불일치.
- **수정 방향:** `event.acer.as_deref().or(event.killer_name.as_deref())` 우선순위.
  `recent_champion_kills` 폴백은 2차로 유지. `make_named_event` 픽스처를 실 피드 shape(`Acer` 키)로 교정.
- **합격기준:** ① acer 우선 ② `{"EventName":"Ace","Acer":"me#TAG"}` 픽스처로 우리 팀 에이스 트리거 발화 테스트
  ③ 적 에이스는 트리거 안 됨 테스트.

### G004 — 게임 종료 에지에 디바운스 없음: 폴링 1회 실패로 녹화 중단 (HIGH, 확인됨 — game_monitor.rs:453 즉시 종료)

- **파일:** `src-tauri/src/recording/game_monitor.rs:235-261, 453` / `live_client.rs:99-112` (`check_live_client_basic` 2초 타임아웃 1회)
- **증상(감사 주장):** 하이브리드 감지가 한 번이라도 `in_game=false`를 내면 즉시
  `on_game_end` → `stop_capture_pipeline`. `check_live_client_basic`은 2초 타임아웃 1회 실패로 `None`
  반환, 그 순간 LCU도 `InProgress`가 아니면 게임 중인데 세션 종료. 연속 실패 임계치 없음.
- **수정 방향:** 종료 전이에 확인 카운터 — 연속 N회(예: 3, ~3초) `in_game=false`를 관측한 뒤에만
  `on_game_end`. 시작 전이는 현행 유지(빠른 게 좋음). `Reconnect` phase를 `InProgress`와 구분.
- **합격기준:** ① 종료 디바운스 N회 ② 단발 타임아웃 시뮬레이션에서 세션 유지 테스트 ③ E5: 게임 중
  네트워크 순단 후 녹화 계속.
- **codex 주의:** 시작 에지(280)는 빠른 게 맞으니 건드리지 말 것. 종료 에지만 디바운스.

### G005 — 로딩 중 리플레이 오분류 → 그 판 클립 0개 (HIGH, 확인됨 — game_monitor.rs:297-327 시작 에지에서만 판정, 재평가 없음)

- **파일:** `live_client.rs:1646-1697` (`detect_replay_mode`), `game_monitor.rs:304-322, 838-841`
- **증상(감사 주장):** `detect_replay_mode`는 `activePlayer`가 `allPlayers`에 없거나 `level == 0`이면
  `Some(true)`. 게임 시작 직후(로딩 화면 근처) 호출되어 `GameMode::Replay(None)`으로 굳음 → 이후 모든
  이벤트가 "Replay event ignored"로 버려짐. `is_spectating()`은 `start_monitoring` 조기 종료.
  게임 모드는 시작 후 재평가 안 됨.
- **수정 방향:** (a) 로딩 상태(`allPlayers` 비었거나 전원 `level==0`)를 리플레이와 구분 —
  리플레이는 `activePlayer`가 **로스터에 있는데 우리가 조작 불가**한 상태. (b) 또는 리플레이 판정을
  게임 시작 후 몇 초 지연(로딩 완료 후). (c) 게임 모드를 첫 유효 데이터 수신 시 1회 재평가.
- **합격기준:** ① 로딩 픽스처(빈 allPlayers)가 리플레이로 굳지 않음 ② 실제 관전 픽스처는 여전히
  리플레이로 판정 ③ E5: 느리게 로딩되는 판에서 클립 생성됨.

### G006 — 자동 감지 in-flight 레이스: GameEnd 직전 한타 클립 유실 (HIGH, 확인됨 — game_monitor.rs:409 detached spawn, :461 abort는 모니터만)

- **파일:** `game_monitor.rs:400-435` (특히 라인 409 `tokio::spawn`), `auto_clip_manager.rs:881-915` (`InflightGuard` 893, 태스크 내부)
- **증상(감사 주장):** 게임 종료 배리어(`wait_for_inflight_clip_tasks`)는 `inflight_clip_tasks` 카운터로
  진행 중 추출을 기다림. 수동 경로는 spawn **전에** 콜백 스레드에서 `InflightGuard` 생성(2527, 주석이 이유).
  기본 경로인 **자동 감지**는 `game_monitor`가 이벤트당 태스크를 바로 spawn하고 카운터 증가는
  태스크 내부에서. `tokio::spawn`과 태스크 폴링 사이 창에서 배리어가 0을 관측 → `stop_recording` →
  뒤늦은 저장이 "녹화가 진행 중이 아닙니다"로 실패 → **GameEnd 직전 한타(최고가치) 유실.**
- **수정 방향:** `game_monitor`가 `tokio::spawn` **전에** (동기 컨텍스트에서) `InflightGuard`를 만들어
  `move`로 태스크에 넘긴다(수동 경로와 동일 패턴). `handle_game_event` 내부의 guard 생성은 제거하거나
  이미 넘어온 guard를 받도록.
- **합격기준:** ① spawn 전 카운트 증가 ② spawn 직후 즉시 `wait_for_inflight_clip_tasks` 호출해도
  그 태스크를 기다림을 테스트 ③ E5: GameEnd 직전 킬이 클립으로 저장됨.
- **codex 주의:** 착수 전 재확인. 이건 동시성 버그라 테스트 설계 신중히 (Claude와 접근 상의 가능).

### G012 — `secs_before_game_end`(MatchPoint 배수)가 프로덕션에서 안 채워짐 (MEDIUM)

- **파일:** `src-tauri/src/recording/highlight_score.rs:126, 280`, `live_client.rs:1806-1807` (주석만, 구현 없음)
- **증상:** 프로덕션 write가 테스트(`auto_clip_manager.rs:2372`)에만 존재. `capture_moment`는
  `secs_before_game_end: None` 하드코딩 → MatchPoint 점수 배수가 절대 적용 안 됨. `moment`/`result`
  유실과 동일 클래스.
- **결정 (2026-09-03, 대표님):** **실제 계산 배선.** GameEnd 이벤트 수신 시점에 각 미저장/저장
  클립의 `event.event_time` 대비 게임 종료 시각으로 `secs_before_game_end`를 역산해 채운다.
  (게임 길이는 GameEnd 이벤트의 `event_time` 또는 캐시 `game_time`.)
- **합격기준:** ① 프로덕션 경로에서 값이 채워짐 ② 그 값이 실제 점수를 움직임을 outcome 테스트로 고정
  (전달만 확인 금지 — "MatchPoint 배수 적용된 점수 ≠ 미적용 점수").

---

## P1 — 실게임을 진단 가능하게 (피드백/관측)

### G007 — `<video>`에 `onError` 없음: 검은 화면 + 무피드백 (HIGH)

- **파일:** `src/components/editor/VideoPreview.tsx:160-168`, `src/components/results/ResultsViewer.tsx:419-425, 489-494`,
  `src/components/editor/auto-edit/AutoEditStoryboard.tsx:227-233, 268-272`, `src/components/results/ClipVault.tsx:1055-1060`
- **증상:** 프리뷰 `<video>`에 `onError` 핸들러가 없다. H.265로 녹화된 클립은 WebView2가 HEVC 디코더가
  없어 재생 불가(설정 기본이 H.264로 바뀐 이유, `models.rs:515-519`) → 검은 프레임, 컨트롤은 살아 있고,
  피드백 0. 누락/차단된 파일도 동일. (`ClipVault` 그리드 `<img>`는 `onError`가 있음 — 불일치.)
- **수정 방향:** 네 곳의 프리뷰 `<video>`에 `onError` → 사용자 메시지("이 클립은 앱에서 미리볼 수 없습니다 —
  파일이 없거나 코덱(H.265)을 지원하지 않습니다. 파일 열기/다시 시도"). `VideoPlayer.tsx:175`가 이미
  `error` 이벤트 → toast 패턴을 쓰므로 그걸 참조.
- **합격기준:** ① 네 프리뷰에 onError+메시지 ② 존재하지 않는 src로 렌더 시 메시지 표시 테스트.

### G008 — 설정 코덱 "AV1"이 가짜 스위치 (MEDIUM)

- **파일:** `src/components/settings/VideoSettings.tsx:371-394`, `src-tauri/src/settings/models.rs:496, 559`,
  `src-tauri/src/recording/integration_backend/mod.rs:40-44`
- **증상:** `VideoCodec::Av1`이 열거형에 있고 UI에서 선택 가능하나, 런타임 매핑은
  `if is_h265() { H265 } else { H264 }`이고 `is_h265()`는 `H265`에만 true → **AV1 선택 시 조용히 H.264 녹화.**
- **결정 (2026-09-03, 대표님):** **(a) UI에서 AV1 제거.** `VideoSettings.tsx`의 코덱 선택지에서 AV1
  옵션 제거. 열거형 `VideoCodec::Av1`은 남겨도 되나 UI 도달 불가. 매핑 안 되는 코덱은 선택 불가.
- **합격기준:** ① UI 코덱 선택지 = {H.264, H.265}만 ② 선택된 코덱과 실제 인코더가 일치하는지
  테스트로 매핑 대조 (미래 회귀 방지).

### G009 — temp_dir 폴백 경로가 `assetProtocol.scope` 밖 (MEDIUM)

- **파일:** `src-tauri/src/main.rs:82, 126-135, 250-266`, `src-tauri/src/utils/media_staging.rs:72-73, 115`, `tauri.conf.json:44-48`
- **증상:** storage/recordings 디렉터리가 권한 실패 시 `std::env::temp_dir().join("lolshorts-recovery"/"...recordings")`로
  폴백. 이 경로는 `assetProtocol.scope`(`$DATA/lolshorts/**`) 밖 → 폴백 활성 시 모든 `convertFileSrc`
  썸네일/프리뷰가 조용히 로드 실패(+ G007의 onError 없음 → 검은 화면). 또한 `media_staging`이 Windows에서
  `\\?\C:\...` verbatim 접두 경로를 반환 → scope glob이 거부 가능(import 프리뷰 안 뜸).
- **수정 방향:** (a) 폴백 디렉터리도 scope에 포함(`$TEMP/lolshorts-*/**` 추가), 또는
  (b) 폴백 활성 시 사용자에게 명시 경고 + 원인·복구 동선(readiness/diagnostics). (c) `media_staging` 반환
  경로에서 `\\?\` 접두 제거(`dunce::canonicalize` 등).
- **합격기준:** ① 폴백 경로에서도 프리뷰 로드되거나 명확한 경고 ② staged 미디어 경로에 `\\?\` 없음.

### G010 — YouTube 토큰 회전 미영속화 (HIGH)

- **파일:** `src-tauri/src/youtube/oauth.rs:169-220, 238-250`, `src-tauri/src/youtube/commands.rs:518-531, 1169-1187, 1264-1267`
- **증상:** `refresh_token`이 `self.credentials` 인메모리 RwLock만 갱신. 업로드마다
  `clone_with_credentials`로 임시 클라이언트 생성, 업로드 후 저장 호출 없음. `save_credentials_for_user`는
  최초 OAuth에서만 호출. 결과: (a) 매 업로드가 만료 토큰으로 시작해 불필요한 refresh 1회,
  (b) Google이 refresh token 회전 시 새 값이 임시 클라이언트와 함께 폐기 → 저장된 refresh token이
  무효화되어 **사용자가 재로그인해야 복구.** 또 `youtube_get_auth_status`는 자격증명 존재 여부만 봐서
  revoke된 계정도 "연결됨"으로 표시(`commands.rs:1140`).
- **수정 방향:** `get_valid_token`/`refresh_token` 성공 직후 `save_credentials_for_user` 호출(회전된
  refresh token 포함). refresh 실패(`invalid_grant`) 시 저장 자격증명 정리 + `youtube_get_auth_status`가
  유효성 반영. (선택) 로그아웃 시 Google `revoke` 엔드포인트 호출.
- **합격기준:** ① refresh 후 키링에 재기록됨 테스트 ② invalid_grant 시 자격증명 정리 + UI가 "연결 필요"로.

### G011 — OBS-01/02: 개인정보 안전 진단 + E5 측정 export (마스터 백로그 P1, 실게임 레버리지)

- **왜:** 실게임 1판이 최대 데이터를 내도록. 지금은 캡처 backend/mode·성능이 로그에 흩어져 있고
  redaction·집계 export가 없다.
- **수정 방향:** 세션당 `capture_backend`(ddagrab/gdigrab), `capture_mode`, median FPS, dropped frames,
  p95 bitrate, RSS, 저장 지연 p95, 에러 코드만 수집(경로·토큰·영상명·user id **미수집**). 게임 종료 시
  redacted 진단 번들 + 성능 CSV/JSON 원클릭 export. 마스터 백로그 §6 "90분 성능 합격 기준" 표의 지표에 맞춰서.
- **합격기준:** ① 수집 필드에 PII 없음(테스트) ② export 번들에 토큰/경로/영상명 없음
  (`test-field-evidence-tools.ps1` 확장) ③ E5에서 실제 번들 생성.

---

## 제품 결정 (2026-09-03 확정)

| ID | 결정 |
|---|---|
| **G008** | **UI에서 AV1 제거** (위 G008 참조) |
| **R001** | **언어 선택기를 ko/en으로 축소.** `src/i18n.ts` 언어 목록 + `LanguageSelector`를 ko/en만.
  나머지 로케일 파일은 남겨도 되나 선택 불가. `fallbackLng: "en"` 유지. |
| **G012** | **실제 계산 배선** (위 G012 참조) |
| R003 | YouTube 콜백 포트 9090 점유 시 명확한 에러 메시지 + 안내 (동적 포트는 Google Web client라 불가). P1 범위. |
| R002 | YouTube 예약 업로드 UI에 "앱 실행 중이어야 함 / 예약 시각에 즉시 공개" 명시. 네이티브 `publishAt` 전환은 E5 후. |

R001을 새 goal로: `G013 언어 선택기 ko/en 축소`.

---

## codex 착수 순서 (2026-09-03 확정: P0 + P1 전부)

전체 스펙(G001~G013)을 codex가 병렬로. **권장 순서** (Claude 항목별 리뷰 후 main 통합):

1. **필드 교체 (작음, 독립):** G001, G003 — 한 필드 읽는 위치 교체 + 픽스처 교정
2. **관대한 파싱:** G002 — `Events` per-event 파싱
3. **점수 배선:** G012 — `secs_before_game_end` 역산
4. **상태 기계 (신중, 동시성):** G004 → G005 → G006 순. G006은 테스트 설계 시 Claude와 상의
5. **프론트 (병렬 가능):** G007(video onError), G008(AV1 제거), G013(언어 축소), G009(assetProtocol 폴백)
6. **YouTube:** G010(토큰 회전 영속화)
7. **관측:** G011(OBS 진단·측정 export) — 마지막, E5 직전에 준비되면 됨

각 항목 완료 시 `.ksi` 원장 `attempt`/`gate` 갱신은 Claude가.

---

## P2 — E5 이후 (참조만, 지금 착수 금지)

- YouTube: 스케줄러 `Running` 고착 복구·직접 업로드 취소·쿼터 403 전용 처리·credential lock 축소·업로드 경로 allowlist
  (감사 상세: youtube 감사 §3)
- 게임 상태 bool→enum 리팩터 (마스터 백로그 §8 모듈 분리에 포함)
- wall-clock 앵커 → monotonic (시계 조정 취약, low)
- i18n 파일 CI 대조 (ko/en 키 드리프트 감지)
- 로그: `EnvFilter::from_default_env()` → 명시적 max level (RUST_LOG=trace로 Authorization 노출 가능, low)
