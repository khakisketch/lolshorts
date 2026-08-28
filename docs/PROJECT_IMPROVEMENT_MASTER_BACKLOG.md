# LoLShorts 전체 개발 개선 마스터 백로그

> 기준일: 2026-08-28
>
> 목표: 개인 일상 사용이 가능하고, Windows 11 x64 + NVIDIA 환경에 무료 공개할 수 있는 안정판 완성
>
> 상태: 코드 기능은 상당 부분 구현됐지만, 현재는 통합·서비스 소유권·현장 검증·서명 배포가 남은 공개 미리보기 후보

이 문서는 LoLShorts의 현재 상태와 앞으로 해야 할 일을 한곳에서 관리하는 기준 문서다. 과거 계획 문서는 역사적 맥락으로만 사용하고, 새 작업의 우선순위와 출시 판단은 이 문서를 따른다.

## 1. 완료를 판단하는 다섯 단계

LoLShorts에서는 다음 상태를 서로 구분한다.

1. **구현됨**: 코드 경로가 존재한다.
2. **자동 검증됨**: 단위·통합·정적 검사에서 통과했다.
3. **E4 데스크톱 검증됨**: 실제 Tauri 설치/실행 환경에서 동작했다.
4. **E5 실게임 검증됨**: 실제 League, GPU, 오디오, 리플레이 환경에서 합격했다.
5. **릴리스 검증됨**: 서명 설치, 업데이트, 롤백, 제거와 두 번째 PC 검증까지 통과했다.

자동 테스트가 녹색이어도 E4/E5와 릴리스 검증을 대신하지 않는다. 두 번째 PC E5 전에는 안정판이 아니라 `public preview`로만 표시한다.

## 2. 현재 기준선

### 로컬 작업 트리

다음 수치는 작업을 시작하기 전에 기록한 **2026-08-28 감사 스냅샷**이다. 이후 상태 판단은 이 숫자를 고정값으로 간주하지 말고 새 inventory와 `git status`를 다시 기록한다.

- 현재 브랜치: `codex/public-release-readiness`
- 현재 HEAD: `91a4e25a1bcbb539cf4891afb74bb68884e05baf`
- 상태 항목 약 381개, tracked 변경 328개, staged 24개, untracked 46개
- tracked diff 규모 약 `+29,025/-14,145`
- 사용자의 기존 변경과 여러 작업 흐름이 한 작업 트리에 함께 있으므로 reset, checkout 복원, clean을 하면 안 된다.
- 개발 서버와 Vite/Tauri 개발 프로세스는 현재 꺼져 있다.

### 원격 GitHub

- 저장소: `khakisketch/lolshorts`
- 기본 브랜치: `main`
- 원격 `main`과 현재 로컬 브랜치는 서로 갈라져 있다. 원격 전용 14개, 로컬 전용 11개 커밋이 있다.
- GitHub 릴리스와 태그는 아직 없다.
- 2026-08-23 KST 감사 시점의 최신 원격 `main` 일반 CI는 성공했지만, 직전 `Release Readiness` [run 32586515341](https://github.com/khakisketch/lolshorts/actions/runs/32586515341)는 실패했다.
- 이 실행의 Playwright 결과는 72 실패, 48 통과, 9 제외였다. 주된 실패는 에디터/오디오/자동편집 E2E의 locator·대기 계약이었다.
- 실패 후 설치판 fixture 검증과 artifact 업로드가 건너뛰어져 설치판 증거가 남지 않았다.
- `production-release` GitHub Environment가 없고, `main` 브랜치 보호도 없다.
- 코드와 workflow에는 Supabase/YouTube/updater 구성 상태를 다루는 경로가 있다. 다만 실제 GitHub production Environment에는 Supabase 공개 클라이언트 구성 외의 YouTube, Tauri updater 서명, Authenticode 값이 아직 연결·검증되지 않았다.

원격 readiness 실패는 현재 dirty checkout의 최신 수정분을 검증한 결과가 아니다. 반대로 로컬 테스트 통과도 현재 원격 `main`의 릴리스 성공을 뜻하지 않는다.

### Supabase

- 로컬 공개 설정이 가리키는 프로젝트는 현재 접근 가능하다.
- Auth 설정, `user_profiles`, `license_tiers`, `auto_edit_usage`, `auto_edit_quota_consumptions` REST 엔드포인트가 응답한다.
- `quota` Edge Function이 응답한다.
- `billing` Edge Function은 배포되지 않았으며, 이는 결함이 아니라 무료 공개판에서 명시적으로 연기한 범위다.
- 하지만 현재 연결된 Supabase 관리 계정에서는 이 LoLShorts 프로젝트가 보이지 않는다.
- 따라서 사용 중인 프로젝트의 소유권·migration history·advisor 결과·운영 접근 권한은 아직 검증하지 못했다.
- 마이그레이션은 `supabase/migrations/**`가 기준이다. 과거 `supabase/schema.sql`은 오적용 위험이 있는 레거시 사본이다.

즉, public anon 경로의 **접속 가능**은 확인됐지만 **운영 관리 가능**은 확인되지 않았다. SB-02부터 SB-04까지는 SB-01에서 프로젝트 관리 권한을 확보하기 전에는 진행할 수 없다.

### 자동 검증 증거

2026-08-27 로컬 검증에는 다음 결과가 있다.

- 프런트 61 suite / 477 test 통과
- ESLint, TypeScript typecheck, production build 통과
- Rust 738 test 통과, 3개 ignored
- Rust fmt와 Clippy 통과
- 실제 FFmpeg 회귀 테스트와 release contract 통과
- release workflow/installer 구성 계약 검사 통과
- Supabase quota 로직 테스트 통과
- npm audit 알려진 취약점 0건
- `cargo audit` 기준 RustSec 취약점 0건, 유지보수 중단 경고 17건 허용

현재 checkout의 Playwright 전체 브라우저 검증은 개발 서버를 다시 켜지 않기 위해 이번 감사에서 재실행하지 않았다.

## 3. 이미 구현되어 다시 만들 필요가 없는 영역

다음 항목은 새로 설계하기보다 통합·회귀 검증·현장 검증에 집중한다.

- 모든 데스크톱 경로의 접힌 사이드바와 명시적 펼치기/접기
- 게임 중심 라이브러리, 게임 아코디언, 레거시 `/games`와 `?tab=games` 호환
- 펼친 게임만 클립 DOM·썸네일 큐를 만드는 초기 렌더 최적화
- 게임 검색·모드 필터·선택 유지·삭제·하이라이트 흐름
- FFmpeg `q` 종료, 5초 종료 예산, 앱이 소유한 프로세스만 종료하는 경로
- 녹화 복구, 출력 검증, durable media job, 중단·재개·실패 상태
- readiness wizard, autostart 동기화, 저장공간 예측·경고
- Supabase/YouTube/updater의 공개 구성 상태를 값 대신 상태·오류 코드로 노출하는 경로
- optional Sentry와 익명 telemetry opt-out. Sentry는 안정판의 필수 조건이 아니다.
- 자동 삭제 기본 비활성화와 무료판 로그인 정책

## 4. P0 — 개발과 배포를 막는 작업

P0를 모두 끝내기 전에는 기능 추가, 대규모 리팩터링, 안정판 태그를 진행하지 않는다.

| ID | 작업 | 실행 내용 | 합격 기준 | 외부 준비 |
|---|---|---|---|---|
| INT-01 | 원본 보존 | 현재 dirty tree의 상태·diff·untracked 목록과 안전 기준점을 보존한다. 기존 변경은 reset하지 않는다. | 원본 checkout이 그대로 있고 복구 가능한 inventory가 있다. | 없음 |
| INT-02 | 통합 기준선 생성 | 최신 원격 `main`에서 별도 worktree와 `codex/` 통합 브랜치를 만들고 로컬 11개 커밋과 dirty 변경을 작업 흐름별로 적용·검토한다. | 원격 14개와 로컬 기능이 한 선형 후보 브랜치에 합쳐지고, 누락·중복 커밋이 없다. | 없음 |
| INT-03 | 변경 묶음 분리 | 녹화/수명주기, 미디어/저장소, UX, 서비스/배포, 테스트/문서 단위로 검토 가능한 커밋을 만든다. | 각 커밋이 독립 설명과 검증 결과를 갖고 비밀·생성물을 포함하지 않는다. | 없음 |
| TEST-01 | 현재 E2E 안정화 | Playwright가 5181 Vite dev server를 직접 띄우는 구조를 통제하고, 남아 있는 `networkidle`, 고정 timeout, 취약한 텍스트 locator를 의미 기반 상태와 접근성 locator로 교체한 뒤 Chromium부터 검증한다. | 현재 통합 커밋에서 Chromium E2E 전부 통과하고 실패 시 dev server·자식 프로세스가 남지 않는다. | 없음 |
| TEST-02 | 전체 게이트 | lint, typecheck, build, Jest, Chromium/Firefox/Edge, Rust fmt/clippy/test, FFmpeg 회귀, Supabase 테스트, release contract를 clean checkout과 통합 checkout에서 실행한다. | 모든 필수 gate가 녹색이며 skipped 항목에 이유와 별도 E5 추적 ID가 있다. | 없음 |
| CI-01 | readiness 분할 | 프런트, Rust, E2E, 미디어/Supabase, installer fixture를 독립 job으로 나누고 캐시·동시성 취소를 적용한다. | 한 job 실패가 다른 진단 artifact를 없애지 않고, 실패 원인이 10분 이내 식별된다. | 없음 |
| CI-02 | 실패 artifact 보존 | 로그, Playwright trace, screenshot, test report, installer fixture를 성공 여부와 무관하게 업로드한다. | `if: always()` 성격의 업로드가 동작하고 비밀은 redaction된다. | 없음 |
| GH-01 | `main` 보호 | 필수 CI, PR 검토, force-push·삭제 차단을 설정한다. | 보호 규칙을 우회하지 않고 통합 PR만 merge 가능하다. | 저장소 관리자 권한 |
| GH-02 | production 환경 | `production-release` Environment를 만들고 승인자·배포 보호를 설정한다. | 일반 CI에서 production secret을 읽을 수 없고 승인된 release만 접근한다. | 저장소 관리자 권한 |
| SB-01 | 운영 프로젝트 확정 | 현재 접근 가능한 hosted project의 관리 권한을 회복하거나, 현재 계정 조직에 새 LoLShorts 프로젝트를 만들고 데이터를 전환한다. | 한 프로젝트가 staging/production의 명시적 기준이며 소유자와 복구 담당이 기록된다. | Supabase 소유자 선택 |
| SB-02 | Supabase 링크·드리프트 검사 | SB-01 완료 후 CLI로 project link, migration list, staging `db push`, schema diff를 수행한다. | hosted migration history와 저장소 migration이 일치한다. | 프로젝트 관리 토큰/권한 |
| SB-03 | RLS·권한 통합 테스트 | SB-01 완료 후 anon/authenticated/service-role별 허용·거부 테스트와 quota 동시성·idempotency 테스트를 추가한다. | 다른 사용자 행 쓰기, quota 직접 조작, 권한 없는 RPC 실행이 모두 거부된다. | staging project와 관리 권한 |
| SB-04 | Advisor·타입 생성 | SB-01 완료 후 security/performance advisor를 처리하고 DB 타입을 생성해 프런트 계약과 비교한다. | high/critical advisor 문제가 0이고 생성 타입 drift가 없다. | staging project와 관리 권한 |
| SB-05 | 레거시 schema 사고 방지 | `supabase/schema.sql`을 archive로 이동하거나 생성 파일로 명시하고, migration 외 SQL 적용을 CI에서 막는다. | 신규 개발자가 레거시 schema를 운영 DB에 적용할 가능성이 제거된다. | 없음 |
| REL-01 | 공개 구성 주입 | Supabase 공개 구성, YouTube desktop OAuth, updater/signing 구성을 production workflow에 주입하고 누락 시 build를 중단한다. | `.env` 없는 clean PC 설치판에서 configured 상태가 정확하고 secret 값이 로그에 없다. | 아래 준비 목록 참조 |
| SEC-01 | 공급망 기본선 | 활성 workflow action을 commit SHA로 고정하고 Dependabot 보안 업데이트와 CodeQL을 활성화한다. | PR에서 dependency/action 변경이 검토 가능하고 high/critical 결과가 gate된다. | 저장소 관리자 권한 |

## 5. P1 — 게임 실행 전에 끝낼 개발 개선

| ID | 작업 | 합격 기준 |
|---|---|---|
| E4-01 | 실제 Tauri shell smoke | 홈·라이브러리·스튜디오·설정·온보딩 경로가 실제 IPC와 함께 열리고 브라우저 mock 의존이 없다. |
| E4-02 | sidecar와 degraded state | FFmpeg/ffprobe 존재, 누락, 손상 상태가 각각 정상·명확한 중단 상태로 표시된다. |
| E4-03 | 설정 지속성 | autostart, 저장 위치, 캡처 설정, telemetry 선택이 재시작 뒤 OS 상태와 일치한다. |
| E4-04 | 복구 시나리오 | 잠긴 출력, 저용량, SQLite 손상 복사본, 처리 중 강제 종료가 복구되거나 사용자 조치가 명확한 terminal state에 도달한다. |
| UX-01 | 오류 경로 E2E | 오디오 믹서, 자동편집, Canvas editor, YouTube, 업데이트의 실패·재시도·취소 흐름을 검증한다. |
| UX-02 | 접근성 확대 | 기존 viewport overflow·Axe·설정/updater 검사를 기반으로 Results/ClipVault, editor, onboarding의 keyboard, focus, `aria-expanded`, dialog, 44px touch target 상호작용 검사를 확대한다. |
| OBS-01 | 개인정보 안전 진단 | 세션별 capture mode, FPS/drop, bitrate, 메모리, 저장 지연, 오류 코드만 수집하고 경로·토큰·영상명은 redaction한다. |
| OBS-02 | E5 측정 export | 게임 종료 후 한 번에 redacted 진단 bundle과 성능 CSV/JSON을 내보내도록 한다. |
| TEST-03 | ignored 테스트 정리 | 빈 FFmpeg integration placeholder 3개는 실제 fixture 테스트로 바꾸거나 중복이면 제거한다. Windows 성능/stress ignored 테스트는 E5 명령으로 문서화한다. |
| REL-02 | installer fixture 검증 | MSI/NSIS에 FFmpeg/ffprobe, 권한, updater endpoint, product metadata가 포함되는지 CI에서 검사한다. |

## 6. E5 — 사용자가 게임하면서 검증할 항목

다음은 코드 리뷰나 mock E2E로 완료 처리할 수 없다.

### 기능 시나리오

- 앱 먼저 실행 / League 먼저 실행
- LCU 연결, 재접속, 게임 시작·종료
- 킬·데스·어시스트 등 이벤트 감지와 클립 생성
- 리플레이 목록, 다운로드, 실행, 타겟 선택 녹화
- 시스템 오디오와 선택 마이크의 실제 녹음·동기화
- overlay가 사용자에게 보이지만 모든 녹화 프레임에서는 제외됨
- 캡처 제외 실패 시 overlay를 숨기는 fail-closed 동작
- 앱 종료 후 5초 안에 LoLShorts 소유 프로세스만 종료되고 다른 FFmpeg는 유지됨

### 90분 성능 합격 기준

Desktop Duplication/NVENC와 GDI fallback을 각각 측정한다.

| 지표 | 합격 기준 |
|---|---:|
| median capture FPS | 59 이상 |
| dropped frames | 1% 미만 |
| League median FPS 저하 | 3% 이하 |
| p95 bitrate | 27.5 Mbps 이하 |
| VMAF | 95 이상 |
| 클립 저장 p95 | 5초 이하 |
| 앱 + FFmpeg RSS | 1.1 GiB 이하 |
| warm-up 이후 메모리 증가 | 15% 이하 |
| black/freeze/decode error | 0건 |

### 클립 품질 합격 기준

- 최소 2게임, 30개 이상 클립을 보관 가치·중복·앞뒤 잘림·오분류·영상·오디오로 라벨링한다.
- 보관 가치 70% 이상, 중복 10% 이하, 앞뒤 잘림 5% 이하를 만족한다.
- 같은 실패 패턴이 3회 이상 반복될 때만 랭킹, 병합 창, pre/post 길이를 조정한다.

## 7. P1 — 공개 릴리스 완성

| ID | 작업 | 합격 기준 |
|---|---|---|
| YT-01 | YouTube 실서비스 | clean PC에서 OAuth, 비공개 업로드, token refresh, sign-out, 취소·재시도가 동작한다. |
| UPD-01 | beta → RC 업데이트 | 서명된 `1.2.0-beta.1`에서 `1.2.0-rc.1`로 실제 업데이트·재시작된다. |
| UPD-02 | 롤백·제거 | 제거, 이전 버전 재설치, 사용자 데이터 보존/삭제 선택, updater 실패 복구를 확인한다. |
| REL-03 | 두 PC 검증 | 현재 RTX 4060 PC와 별도의 clean Windows 11 + NVIDIA PC에서 E5 정식 행이 모두 Pass다. |
| REL-04 | 안정판 게시 | 서명 MSI/NSIS, updater manifest, checksum, 변경 내역, 지원 범위를 GitHub Release로 게시한다. |
| REL-05 | 7일 관찰 | 치명적 녹화·업데이트·OAuth·종료 오류가 없고 오류 코드 추세가 안정적이다. 이후 RC 표시를 제거한다. |

## 8. P2 — RC 이후 구조·성능·유지보수 개선

대규모 모듈 분리는 지금의 통합과 E5를 지연시키지 않도록 행동을 고정한 뒤 진행한다.

### Rust 모듈 분리 후보

- `segment_recorder.rs` 약 3,530줄: 프로세스 수명주기, 세그먼트 인덱스, 검증, 복구 분리
- `storage/mod.rs` 약 3,237줄: 게임/클립/작업/설정 repository 분리
- `auto_clip_manager.rs` 약 2,661줄: 이벤트 수집, 점수, 병합, persistence 분리
- `live_client.rs` 약 2,504줄: transport, reconnect, DTO mapping 분리
- `video/commands.rs` 약 2,419줄과 `processor/pipeline.rs` 약 2,128줄: IPC façade와 pipeline stage 분리
- `youtube/commands.rs` 약 2,071줄: OAuth, token store, upload session, command façade 분리
- `recording/commands.rs` 약 1,911줄: 설정, start/stop, diagnostics 분리
- `main.rs` 약 1,135줄: plugin/command/state registration 모듈화

분리 중 Tauri command 이름, 이벤트 payload, SQLite 형식과 저장 경로는 바꾸지 않는다.

### 프런트 모듈 분리 후보

- `ClipVault.tsx` 약 1,085줄: query/filter controller, game accordion, player, selection action bar 분리
- `BasicSettings.tsx` 약 963줄: readiness, recording, storage, service status section 분리
- 서버 상태는 query/cache 계층, 편집 중 로컬 상태는 Zustand/component state로 역할을 명확히 한다.
- 큰 목록은 현재의 접힌 그룹 lazy rendering을 유지하고, 측정에서 필요할 때만 virtualization을 도입한다.

### 의존성 개선

- 먼저 React 18/Tauri 2 현재 계열에서 patch/minor 업데이트를 작은 묶음으로 적용한다.
- Supabase JS, Playwright, Sentry, Radix, i18n 업데이트마다 계약 테스트를 실행한다.
- React 19, Vite 8, Tailwind 4, Jest 30, ESLint 10, TypeScript 7은 별도 migration으로 미룬다.
- 중복 `reqwest`, Windows crate 세대는 직접 의존성 영향부터 분석하고, binary 크기·compile time·runtime 지표가 개선될 때만 정리한다.

### 문서 정리

- `WORKSTREAM_STABILIZATION.md`, `PRODUCTION_COMPLETION_EXECUTION_PLAN.md`, `PRODUCTION_STATUS.md`, `PERFORMANCE_VALIDATION.md`의 오래된 수치와 경로를 역사 문서로 명확히 표시한다.
- 이 문서에서 E4/E5, release runbook, Supabase migration 문서로 연결한다.
- 지원 플랫폼, 무료판 로그인 정책, 제외 기능을 README와 사용자 가이드에서 일치시킨다.

## 9. 사용자 또는 외부 계정에서 준비할 것

비밀 값은 채팅, Git 기록, 진단 bundle에 붙여 넣지 않는다. GitHub Environment secret 또는 해당 서비스의 보안 저장소에만 넣는다.

### Supabase

- 현재 앱이 가리키는 프로젝트의 소유자 초대 또는 새 LoLShorts 프로젝트 생성 결정
- staging과 production 프로젝트 구분 여부 결정
- CLI link/deploy가 가능한 관리 권한
- email Auth redirect URL과 production origin 확정

### YouTube/Google Cloud

- Desktop OAuth client ID
- 현재 구현과 일치하는 redirect URI
- OAuth consent screen과 test/production 사용자 정책
- 비공개 업로드를 수행할 테스트 YouTube 채널
- client secret은 public desktop app에서 완전한 비밀로 간주할 수 없으므로 scope와 사용량 제한을 함께 관리

### GitHub/서명

- `production-release` Environment 생성 권한
- Tauri updater private key와 password, 앱에 포함할 public key
- Windows Authenticode 인증서와 password
- GitHub Release 게시 권한
- `main` 보호 규칙을 설정할 관리자 권한

### E5 장비

- RTX 4060 현재 PC와 별도 clean Windows 11 + NVIDIA PC
- 충분한 디스크 여유 공간과 온도/GPU 측정 도구
- 실제 League 계정, 리플레이, 최소 2게임 시간
- Discord/OBS/브라우저 등 오디오 충돌 가능 앱을 포함한 현실적인 사용 환경

Sentry DSN은 선택 사항이다. 사용하지 않아도 로컬 redacted diagnostics와 익명 오류 코드만으로 출시할 수 있다.

## 10. 실행 순서

1. **통합 기준선**: INT-01 → INT-02 → INT-03
2. **자동 게이트 복구**: TEST-01 → TEST-02 → CI-01 → CI-02
3. **원격 보호와 서비스**: GH-01/02, SB-01 → SB-05, SEC-01
4. **데스크톱 준비**: E4-01 → E4-04, UX-01/02, OBS-01/02, TEST-03
5. **실게임**: E5 기능 → 90분 성능 → 30개 클립 품질
6. **서비스·설치판**: YT-01, REL-01/02, UPD-01/02
7. **공개**: 두 PC E5 → RC → 7일 관찰 → 안정판
8. **후속 최적화**: 모듈 분리, 의존성 major migration, 추가 플랫폼 검토

## 11. 지금 하지 않을 것

- 현재 dirty worktree를 reset, clean, 강제 checkout하지 않는다.
- 서로 갈라진 현재 로컬 브랜치를 그대로 원격 `main`에 force-push하지 않는다.
- E5 전에 캡처·랭킹 알고리즘을 감으로 조정하지 않는다.
- 통합과 E5 전에 React/Vite/Tailwind 등 major upgrade를 하지 않는다.
- 결제, Toss, PRO 판매, billing function을 활성화하지 않는다.
- AMD·Intel·CPU 인코딩을 정식 지원으로 광고하지 않는다.
- macOS/Linux, TikTok/Instagram 직접 업로드를 이번 안정판 범위에 넣지 않는다.
- 두 PC와 서명 업데이트 검증 전 안정판 태그를 만들지 않는다.

## 12. 최종 완료 정의

다음 조건을 모두 만족할 때만 “개발 개선 완료 및 무료 안정판 준비 완료”로 판단한다.

- 최신 원격 `main` 위의 clean 통합 브랜치에서 모든 자동 gate가 녹색이다.
- 운영 Supabase의 소유권, migrations, RLS, grants, advisors가 검증됐다.
- `.env` 없는 clean PC에서 설치·로그인·편집·내보내기·YouTube가 동작한다.
- 실제 League의 녹화, 이벤트, 리플레이, 오디오, overlay 제외, 종료가 E5를 통과한다.
- 90분 성능 기준과 30개 클립 품질 기준을 통과한다.
- 서명 beta → RC 업데이트, 롤백, 제거가 두 PC에서 통과한다.
- 릴리스 artifact와 updater manifest가 GitHub Release에서 실제 다운로드 가능하다.
- 7일 관찰 기간에 치명 오류가 없다.

관련 실행 문서:

- [Public Release Integration Inventory](./PUBLIC_RELEASE_INTEGRATION_INVENTORY.md)
- [E4 Desktop Smoke Packet](./E4_DESKTOP_SMOKE_PACKET.md)
- [E5 Field QA Packet](./E5_FIELD_QA_PACKET.md)
- [Release Field QA Runbook](./RELEASE_FIELD_QA_RUNBOOK.md)
- [Service Readiness Policy](./SERVICE_READINESS_POLICY.md)
