# LoLShorts

<div align="center">

![LoLShorts Banner](docs/images/banner.png)

**리그 오브 레전드 플레이를 자동으로 녹화하고, 쇼츠와 매드무비를 1초 만에 완성하는 AI 비서**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-Windows-blue.svg)](https://www.microsoft.com/windows)
[![Version](https://img.shields.io/badge/version-1.2.0-green.svg)](https://github.com/KhakiSkech/lolshorts/releases)

[기능](#-features) • [다운로드](#-download) • [사용법](#-usage) • [배포 및 업데이트](#-distribution) • [개발](#-development)

</div>

---

## 📖 개요

**LoLShorts**는 단순한 녹화 프로그램이 아닙니다. 당신의 플레이를 분석하고, 가장 빛나는 순간을 찾아내어, **유튜브 쇼츠(Shorts)**와 **롱폼 몽타주(Montage)** 영상으로 자동 제작해 주는 **크리에이터를 위한 필수 도구**입니다.

이제 LoL 클라이언트를 켜지 않아도 전적을 검색하고, 리플레이를 실행하여 **'페이커'의 시점**으로 명장면을 추출할 수 있습니다.

> **Readiness notice:** README의 기능 설명은 제품 방향과 현재 구현 범위를 설명합니다. 실제 LoL/리플레이 동작, YouTube 계정 업로드, GPU/오디오 캡처, Windows 설치/업데이트, 지원 워크플로, 결제/Toss/PRO 정책은 공개 준비 상태로 주장하기 전에 [Field QA 체크리스트](docs/FIELD_QA_COMMERCIAL_READINESS.md)와 [Service Readiness Policy](docs/SERVICE_READINESS_POLICY.md)의 현장 검증 증거가 필요합니다.

> **Auth/Billing authority:** LoLShorts는 로컬 앱 데이터 저장소로 SQLite를 사용하지만, 인증/결제/PRO 권한의 source of truth는 로컬 DB가 아닙니다. Supabase Auth와 Supabase `user_licenses`/`subscriptions`/`payments`가 권한의 authoritative source이며, 현재 Toss/live checkout은 deferred 상태입니다.

---

## ✨ 주요 기능 (v1.2.0)

### 🎯 1. 리플레이 허브 & 타겟팅 녹화 (New)
*   **원스톱 관리:** 앱 내에서 내 전적(최근 20게임)을 조회하고, 리플레이를 바로 다운로드/실행합니다.
*   **스마트 타겟팅:** 리플레이 실행 시 **"누구를 녹화할까요?"** 팝업이 뜹니다. 원하는 선수(예: Faker)를 선택하면, 카메라가 그 선수를 따라다니며 킬 장면만 골라내어 녹화합니다.

### 🎬 2. 듀얼 포맷 에디터 (New)
*   **Shorts 모드 (9:16)**: 모바일에 최적화된 세로 영상. AI가 챔피언을 중심으로 화면을 자동 크롭합니다.
*   **Montage 모드 (16:9)**: PC/TV 시청용 가로 영상. 여러 하이라이트 클립을 시간 순서대로 매끄럽게 연결하여 **매드무비**를 만듭니다.

### 🤖 3. 지능형 자동 편집 (Auto-Edit)
*   **오늘의 하이라이트**: 오늘 플레이한 5게임을 선택하면, 그중 최고의 장면(펜타킬 > 쿼드라킬...)들만 뽑아 1분짜리 요약 영상을 만듭니다.
*   **중복 방지 시스템**: 한 번 영상으로 만들어진 클립은 다음 편집 시 자동으로 제외되어, 항상 새로운 장면을 보여줍니다.

### ⚡ 4. 강력한 성능
*   **하드웨어 가속**: 1차 정식 지원은 Windows 11 x64 + NVIDIA NVENC입니다. AMD/Intel/CPU 인코딩 경로는 폴백·실험 기능으로 유지되며 동일한 품질을 보증하지 않습니다.
*   **로컬 처리 중심**: 영상 분석과 편집은 사용자 PC에서 처리되도록 설계되어 있습니다. 단, YouTube 업로드, 계정 흐름, 외부 API 연동, 사용자가 제출하는 지원/진단 자료는 선택한 데이터가 기기 밖으로 전송될 수 있습니다.

---

## 📥 다운로드 및 설치

### 시스템 요구 사항
- **정식 지원**: Windows 11 x64 + NVIDIA NVENC
- **실험적 폴백**: AMD/Intel/CPU 인코딩 및 Windows 10은 동작할 수 있지만 이번 공개판의 정식 지원 범위가 아닙니다.
- **League of Legends**: 설치 및 최신 업데이트 필요

두 번째 clean Windows 11 + NVIDIA 장비에서 E5 현장 검증이 완료되기 전 배포물은 **public preview**로 취급합니다.

### 설치 방법
1. [Releases 페이지](https://github.com/KhakiSkech/lolshorts/releases/latest)에서 제공되는 설치 파일을 다운로드합니다. 설치 파일 유형과 서명 상태는 릴리스별로 확인해야 합니다.
   - **MSI Installer (추천)**: `LoLShorts_1.2.0_x64_en-US.msi`
2. 파일을 실행하여 설치합니다. 배포 빌드는 FFmpeg 구성 요소를 포함하도록 설정되어 있지만, 공개 준비 상태는 별도 Windows 설치 검증으로 확인해야 합니다.
3. 바탕화면의 **LoLShorts** 아이콘을 실행합니다.

---

## 🚀 사용 가이드

### Case A: 내가 한 게임 녹화하기 (Live)
1. **LoLShorts 실행**: 앱을 켜두기만 하세요.
2. **게임 시작**: 리그 오브 레전드를 플레이합니다.
3. **자동 녹화**: 킬, 멀티킬, 바론 스틸 등 중요 이벤트가 발생하면 자동으로 녹화되어 저장됩니다.

### Case B: 리플레이로 매드무비 만들기 (Replay)
1. **리플레이 탭**: 앱 좌측 메뉴에서 `Replays`를 클릭합니다.
2. **전적 선택**: 원하는 게임의 `Download` 버튼을 누르고, 완료되면 `Watch`를 클릭합니다.
3. **타겟 선택**: 게임 로딩 후 팝업이 뜨면 **녹화하고 싶은 선수**를 선택합니다.
4. **카메라 고정**: 게임 내에서 해당 챔피언을 **더블 클릭**하거나 **F1~F5** 키를 눌러 시점을 고정하세요.
5. **완성**: 게임이 끝나면 `Editor` 탭에서 추출된 클립들을 모아 **"Export Montage"**를 누르면 매드무비가 완성됩니다.

---

## 🚀 배포 및 자동 업데이트 (Distribution)

LoLShorts는 **GitHub Actions**와 **Tauri Updater** 기반의 배포/업데이트 설정 경로를 제공합니다. 다만 빌드 또는 설정 문서는 공개용 서명 인스톨러, 자동 업데이트, 업그레이드, 롤백, 제거가 검증되었다는 증거가 아니며, 실제 Windows 현장 검증이 필요합니다.

### 업데이트 원리
- 설정된 업데이트 채널이 있을 때 앱은 최신 버전 확인 흐름을 사용할 수 있습니다.
- 새 버전 알림, 다운로드, 설치, 재시작, 롤백 동작은 서명된 배포 채널에서 별도 검증해야 합니다.

### 개발자 배포 가이드
새 버전을 배포하려면 다음 단계를 따르세요:

1. **버전 올리기**: `package.json`과 `src-tauri/Cargo.toml`의 버전을 동일하게 수정합니다.
2. **태그 푸시**:
   ```bash
   git add .
   git commit -m "chore: release v1.2.1"
   git tag v1.2.1
   git push origin v1.2.1
   ```
3. **자동 빌드 설정**: GitHub Actions로 빌드, 서명, 릴리스 생성을 구성할 수 있습니다. 생성된 산출물은 공개 배포 전에 Field QA의 설치/업데이트 검증을 통과해야 합니다.

---

## 🛠️ 기술 스택

- **Core**: Tauri 2.0, Rust (Tokio)
- **Frontend**: React 18, TypeScript, Tailwind CSS, Shadcn/UI
- **Video Engine**: FFmpeg (Sidecar Pattern), Windows Media Foundation
- **Integration**: LCU API (League Client Update), Live Client Data API

---

## 📚 문서

### 사용자 문서

- **[사용자 가이드 (한국어)](docs/USER_GUIDE.md)** - 앱 설치, 사용, 업로드 방법
- **[자동편집 가이드](docs/AUTO_EDIT_GUIDE.md)** - 자동 영상 생성 기능 상세 설명
- **[Canvas 튜토리얼](docs/CANVAS_TUTORIAL.md)** - 오버레이 및 브랜딩 가이드
- **[오디오 믹싱](docs/AUDIO_MIXING.md)** - 배경음악 추가 및 음성 믹싱

### 개발자 문서

- **[개발 가이드](docs/DEVELOPMENT.md)** - 개발 환경 설정 및 빌드 방법
- **[아키텍처](docs/ARCHITECTURE.md)** - 시스템 설계 및 모듈 구조
- **[빌드 가이드](BUILD_GUIDE.md)** - 프로덕션 빌드 및 배포
- **[트러블슈팅](docs/TROUBLESHOOTING.md)** - 런타임 문제 해결
- **[Production Hardening Plan](docs/PRODUCTION_HARDENING_PLAN.md)** - production 제품화까지 남은 개발/검증 작업
- **[Field QA 체크리스트](docs/FIELD_QA_COMMERCIAL_READINESS.md)** - 결제 키 활성화 전 현장 검증 항목
- **[Service Readiness Policy](docs/SERVICE_READINESS_POLICY.md)** - 설치, 업데이트, 지원, 개인정보, FREE/PRO 정책의 검증 기준

---

## 📄 라이선스

이 프로젝트는 MIT 라이선스 하에 배포됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참조하세요.

---

## 🙏 지원

문제가 있거나 피드백을 원하시면:

- **GitHub Issues**: [버그 리포트 및 기능 요청](https://github.com/KhakiSkech/lolshorts/issues)
- **문서**: 위의 "문서" 섹션에서 가이드 참고

<br>

<div align="center">
  Made with ❤️ by the LoLShorts Team
</div>
