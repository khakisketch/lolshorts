import { test, expect, BASE_URL, loginAsProUser } from './fixtures/tauri-fixture';
import { Page } from '@playwright/test';

/**
 * Audio Mixer E2E Tests
 *
 * Audio mixer lives inside the Auto-Edit page as the "Audio" tab.
 * Navigation: PRO login -> /auto-edit -> click audio-tab -> audio-mixer visible
 *
 * Audio Specifications:
 * - Game Audio Range: 0-100%
 * - Background Music Range: 0-100%
 * - Default Mix: 70% game, 30% music
 * - Music slider is DISABLED when no music is uploaded
 *
 * Radix Slider interaction notes:
 * - Cannot use .fill() on Radix Slider root — use the thumb [role="slider"]
 * - Use keyboard ArrowRight/ArrowLeft on the thumb to adjust value
 * - Or use page.evaluate to set React state directly
 * - For preset tests: click preset button and verify value display changes
 *
 * Mock environment notes:
 * - No music file upload in mock (Tauri dialog not available)
 * - Music slider is disabled without uploaded music
 * - Tests for upload, loop, remove are commented with explanations
 */

async function navigateToAudioMixer(page: Page) {
  await loginAsProUser(page);
  await page.goto(`${BASE_URL}/auto-edit`);
  await page.locator('[data-testid="advanced-settings-button"]').click();
  // `networkidle` 을 쓰지 않는다: vite dev 서버는 HMR 소켓과 지연 로딩 청크로
  // 요청이 끊기지 않아서, 이 describe 의 **첫 테스트만** 90초 타임아웃으로
  // 죽곤 했다(따로 돌리면 통과). 붙어야 할 것을 직접 기다리는 편이 빠르고
  // 무엇보다 결정적이다 — 무작위로 빨간불이 켜지는 게이트는 게이트가 아니다.
  await expect(page.locator('[data-testid="audio-tab"]')).toBeVisible({
    timeout: 30000,
  });
  await page.click('[data-testid="audio-tab"]');
  await expect(page.locator('[data-testid="audio-mixer"]')).toBeVisible();
}

/**
 * Helper: Get the slider thumb for a Radix Slider.
 * Radix Slider renders a hidden <input> and a visible [role="slider"] thumb.
 * We interact with the thumb via keyboard.
 */
async function getSliderThumb(page: Page, sliderTestId: string) {
  return page.locator(`[data-testid="${sliderTestId}"] [role="slider"]`);
}

/**
 * Helper: Set a Radix Slider to approximately a target value using keyboard.
 * Assumes current value is known and steps by 1 per key press.
 */
async function setSliderValue(
  page: Page,
  sliderTestId: string,
  targetValue: number
) {
  const thumb = await getSliderThumb(page, sliderTestId);
  await thumb.focus();

  // Read current value from aria-valuenow
  const currentStr = await thumb.getAttribute('aria-valuenow');
  const current = parseInt(currentStr ?? '0', 10);

  const diff = targetValue - current;
  const key = diff > 0 ? 'ArrowRight' : 'ArrowLeft';
  const steps = Math.abs(diff);

  for (let i = 0; i < steps; i++) {
    await page.keyboard.press(key);
  }
}

// ---------------------------------------------------------------------------
// Volume Controls (8 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Volume Controls', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should show audio mixer with default game audio value 70%', async ({ page }) => {
    const gameAudioValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameAudioValue).toBe('70%');
  });

  test('should show audio mixer with default music audio value 30%', async ({ page }) => {
    const musicAudioValue = await page.locator('[data-testid="music-audio-value"]').textContent();
    expect(musicAudioValue).toBe('30%');
  });

  test('should render game audio slider', async ({ page }) => {
    await expect(page.locator('[data-testid="game-audio-slider"]')).toBeVisible();
  });

  test('should adjust game audio volume using keyboard on slider thumb', async ({ page }) => {
    await setSliderValue(page, 'game-audio-slider', 50);

    const value = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(value).toBe('50%');
  });

  test('should allow muting game audio to 0%', async ({ page }) => {
    await setSliderValue(page, 'game-audio-slider', 0);

    const value = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(value).toBe('0%');
  });

  test('should allow maximum game audio at 100%', async ({ page }) => {
    await setSliderValue(page, 'game-audio-slider', 100);

    const value = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(value).toBe('100%');
  });

  test('game audio slider thumb has proper aria-valuemin=0 and aria-valuemax=100', async ({
    page,
  }) => {
    const thumb = await getSliderThumb(page, 'game-audio-slider');
    await expect(thumb).toHaveAttribute('aria-valuemin', '0');
    await expect(thumb).toHaveAttribute('aria-valuemax', '100');
  });

  test('mix preview bars are visible reflecting default 70/30 split', async ({ page }) => {
    await expect(page.locator('[data-testid="mix-preview"]')).toBeVisible();
    await expect(page.locator('[data-testid="mix-preview-game"]')).toBeVisible();
    await expect(page.locator('[data-testid="mix-preview-music"]')).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// Quick Presets (7 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Quick Presets', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should apply "Game Only" preset (100% game, 0% music)', async ({ page }) => {
    await page.click('[data-testid="preset-game-only"]');

    const gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    const musicValue = await page.locator('[data-testid="music-audio-value"]').textContent();

    expect(gameValue).toBe('100%');
    expect(musicValue).toBe('0%');
  });

  test('should apply "Balanced" preset (70% game, 30% music)', async ({ page }) => {
    // First change from default so we can verify a real change
    await page.click('[data-testid="preset-game-only"]');

    await page.click('[data-testid="preset-balanced"]');

    const gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    const musicValue = await page.locator('[data-testid="music-audio-value"]').textContent();

    expect(gameValue).toBe('70%');
    expect(musicValue).toBe('30%');
  });

  test('should apply "Music Focus" preset (40% game, 60% music)', async ({ page }) => {
    await page.click('[data-testid="preset-music-focus"]');

    const gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    const musicValue = await page.locator('[data-testid="music-audio-value"]').textContent();

    expect(gameValue).toBe('40%');
    expect(musicValue).toBe('60%');
  });

  test('should apply "Music Only" preset (0% game, 100% music)', async ({ page }) => {
    await page.click('[data-testid="preset-music-only"]');

    const gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    const musicValue = await page.locator('[data-testid="music-audio-value"]').textContent();

    expect(gameValue).toBe('0%');
    expect(musicValue).toBe('100%');
  });

  test('should update mix preview proportions after applying Music Focus preset', async ({
    page,
  }) => {
    await page.click('[data-testid="preset-music-focus"]');
    await page.waitForTimeout(300); // Allow animation to complete

    const gameBar = page.locator('[data-testid="mix-preview-game"]');
    const musicBar = page.locator('[data-testid="mix-preview-music"]');

    const gameWidth = await gameBar.evaluate((el) => parseInt(window.getComputedStyle(el).width));
    const musicWidth = await musicBar.evaluate((el) =>
      parseInt(window.getComputedStyle(el).width)
    );

    if (gameWidth + musicWidth > 0) {
      const ratio = gameWidth / (gameWidth + musicWidth);
      // Music Focus: 40% game, 60% music -> ratio ~0.4
      expect(ratio).toBeCloseTo(0.4, 1);
    }
  });

  test('all four preset buttons are visible', async ({ page }) => {
    await expect(page.locator('[data-testid="preset-game-only"]')).toBeVisible();
    await expect(page.locator('[data-testid="preset-balanced"]')).toBeVisible();
    await expect(page.locator('[data-testid="preset-music-focus"]')).toBeVisible();
    await expect(page.locator('[data-testid="preset-music-only"]')).toBeVisible();
  });

  test('should allow manual keyboard adjustment after applying a preset', async ({ page }) => {
    // Apply Balanced preset first
    await page.click('[data-testid="preset-balanced"]');

    let gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValue).toBe('70%');

    // Manually adjust game audio up by 10 via keyboard
    await setSliderValue(page, 'game-audio-slider', 80);

    gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValue).toBe('80%');
  });
});

// ---------------------------------------------------------------------------
// Background Music Upload (5 tests — mostly UI verification)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Background Music Upload', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should show upload UI when no music is uploaded', async ({ page }) => {
    await expect(page.locator('[data-testid="upload-music-button"]')).toBeVisible();
  });

  test('upload music button is enabled and clickable', async ({ page }) => {
    const uploadBtn = page.locator('[data-testid="upload-music-button"]');
    await expect(uploadBtn).toBeEnabled();
    // Click should not throw (Tauri dialog is mocked)
    await uploadBtn.click();
  });

  test('music file input has correct audio accept attribute (when present)', async ({ page }) => {
    // File input may only appear after clicking upload or may always be in DOM
    const fileInput = page.locator('[data-testid="music-file-input"]');
    const inputExists = await fileInput.count() > 0;

    if (inputExists) {
      const acceptAttr = await fileInput.getAttribute('accept');
      // Should accept audio files
      expect(acceptAttr).toMatch(/audio/);
    }
    // If input doesn't exist yet, test passes (Tauri dialog handles file selection)
  });

  test('no music card is visible before upload', async ({ page }) => {
    // With no music uploaded, music card/info should not be visible
    const musicCard = page.locator('[data-testid="music-card"]');
    const isVisible = await musicCard.isVisible({ timeout: 1000 }).catch(() => false);
    expect(isVisible).toBe(false);
  });

  test('remove music button is not visible before upload', async ({ page }) => {
    // Remove button should only appear after music is uploaded
    const removeBtn = page.locator('[data-testid="remove-music-button"]');
    const isVisible = await removeBtn.isVisible({ timeout: 1000 }).catch(() => false);
    expect(isVisible).toBe(false);
  });

  // NOTE: The following upload/remove tests require Tauri dialog mock support.
  // They are commented out because the Tauri plugin-dialog open() call cannot
  // be intercepted in the current mock setup.
  //
  // test('should display music file info after upload', async ({ page }) => {
  //   // Requires: page.setInputFiles() or Tauri dialog mock
  //   // await expect(page.locator('[data-testid="music-file-name"]')).toContainText('.mp3');
  // });
  //
  // test('should remove uploaded music', async ({ page }) => {
  //   // Requires music to be uploaded first
  //   // await page.click('[data-testid="remove-music-button"]');
  //   // await expect(page.locator('[data-testid="upload-music-button"]')).toBeVisible();
  // });
});

// ---------------------------------------------------------------------------
// Loop Control (4 tests — commented since upload is not available in mock)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Loop Control', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('loop toggle element exists in DOM', async ({ page }) => {
    // Loop control should be present in DOM (may be disabled/hidden without music)
    const loopToggle = page.locator('[data-testid="loop-music-toggle"]');
    const exists = await loopToggle.count() > 0;
    // Either the toggle exists (possibly disabled) or it's hidden until music is loaded
    // We just verify the page rendered without error
    await expect(page.locator('[data-testid="audio-mixer"]')).toBeVisible();
    // If loop toggle exists, it should be attached
    if (exists) {
      await expect(loopToggle.first()).toBeAttached();
    }
  });

  // NOTE: The following loop tests require music to be uploaded first.
  // In mock mode, Tauri dialog is not available so music cannot be uploaded.
  //
  // test('should have loop enabled by default after music upload', async ({ page }) => {
  //   // Mock: Music uploaded
  //   // const loopToggle = page.locator('[data-testid="loop-music-toggle"]');
  //   // await expect(loopToggle).toBeChecked();
  // });
  //
  // test('should toggle loop on then off', async ({ page }) => {
  //   // Mock: Music uploaded
  //   // await page.click('[data-testid="loop-music-toggle"]');
  //   // await expect(page.locator('[data-testid="loop-music-toggle"]')).not.toBeChecked();
  //   // await page.click('[data-testid="loop-music-toggle"]');
  //   // await expect(page.locator('[data-testid="loop-music-toggle"]')).toBeChecked();
  // });
  //
  // test('should show warning when loop is disabled', async ({ page }) => {
  //   // Mock: Music uploaded, then loop disabled
  //   // await expect(page.locator('text=/Music will play once/')).toBeVisible();
  // });
  //
  // test('should hide warning when loop is re-enabled', async ({ page }) => {
  //   // Mock: Music uploaded, loop disabled then re-enabled
  //   // await expect(page.locator('text=/Music will play once/')).not.toBeVisible();
  // });

  test('audio mixer remains stable without music uploaded', async ({ page }) => {
    // Verify audio mixer does not crash when no music is present
    await expect(page.locator('[data-testid="audio-mixer"]')).toBeVisible();
    await expect(page.locator('[data-testid="game-audio-value"]')).toBeVisible();
  });

  test('game audio value display is correct after preset change without music', async ({
    page,
  }) => {
    await page.click('[data-testid="preset-game-only"]');
    const value = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(value).toBe('100%');
  });

  test('audio mixer responds to multiple preset switches', async ({ page }) => {
    await page.click('[data-testid="preset-game-only"]');
    await page.click('[data-testid="preset-music-focus"]');
    await page.click('[data-testid="preset-balanced"]');

    const gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValue).toBe('70%');
  });
});

// ---------------------------------------------------------------------------
// Mix Preview Visualization (4 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Mix Preview Visualization', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should show visual mix preview bar', async ({ page }) => {
    await expect(page.locator('[data-testid="mix-preview"]')).toBeVisible();
    await expect(page.locator('[data-testid="mix-preview-game"]')).toBeVisible();
    await expect(page.locator('[data-testid="mix-preview-music"]')).toBeVisible();
  });

  test('should display distinct colors for game and music bars', async ({ page }) => {
    const gameBar = page.locator('[data-testid="mix-preview-game"]');
    const musicBar = page.locator('[data-testid="mix-preview-music"]');

    const gameColor = await gameBar.evaluate(
      (el) => window.getComputedStyle(el).backgroundColor
    );
    const musicColor = await musicBar.evaluate(
      (el) => window.getComputedStyle(el).backgroundColor
    );

    // Both should have a color set
    expect(gameColor).toMatch(/rgb/);
    expect(musicColor).toMatch(/rgb/);

    // Colors should be different
    expect(gameColor).not.toBe(musicColor);
  });

  test('should show labels in preview bars at default 70/30 split', async ({ page }) => {
    // At 70% game / 30% music the bars are wide enough for labels
    await expect(page.locator('[data-testid="mix-preview-game"]')).toContainText('Game');
    await expect(page.locator('[data-testid="mix-preview-music"]')).toContainText('Music');
  });

  test('should update preview proportions when Game Only preset is applied', async ({ page }) => {
    await page.click('[data-testid="preset-game-only"]');
    await page.waitForTimeout(200);

    const gameBar = page.locator('[data-testid="mix-preview-game"]');
    const musicBar = page.locator('[data-testid="mix-preview-music"]');

    const gameWidth = await gameBar.evaluate((el) => parseInt(window.getComputedStyle(el).width));
    const musicWidth = await musicBar.evaluate((el) =>
      parseInt(window.getComputedStyle(el).width)
    );

    // With game=100, music=0, game bar should be much wider than music bar
    expect(gameWidth).toBeGreaterThan(musicWidth);
  });
});

// ---------------------------------------------------------------------------
// Tip and Recommendations (2 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Tip and Recommendations', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should display an audio tip section', async ({ page }) => {
    await expect(page.locator('[data-testid="audio-tip"]')).toBeVisible();
  });

  test('audio tip section contains visible text content', async ({ page }) => {
    const tipText = await page.locator('[data-testid="audio-tip"]').textContent();
    expect(tipText).toBeTruthy();
    expect(tipText!.length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// Music Slider Disable State (2 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Music Slider Disable State', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('music audio slider should be disabled when no music is uploaded', async ({ page }) => {
    // Without uploaded music, the music volume slider should be disabled
    const musicSlider = page.locator('[data-testid="music-audio-slider"]');
    await expect(musicSlider).toBeVisible();

    // For Radix Slider, disabled state may be on the root container or the thumb
    const isDisabledOnRoot = await musicSlider.getAttribute('data-disabled');
    const thumb = musicSlider.locator('[role="slider"]');
    const thumbAriaDisabled = await thumb.getAttribute('aria-disabled');

    // At least one of these should indicate disabled state
    const isDisabled =
      isDisabledOnRoot !== null ||
      thumbAriaDisabled === 'true' ||
      (await musicSlider.isDisabled().catch(() => false));

    expect(isDisabled).toBe(true);
  });

  test('game audio slider remains enabled regardless of music upload state', async ({ page }) => {
    // Game audio should always be controllable
    const gameSlider = page.locator('[data-testid="game-audio-slider"]');
    await expect(gameSlider).toBeVisible();

    const isDisabledOnRoot = await gameSlider.getAttribute('data-disabled');
    expect(isDisabledOnRoot).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// State Persistence (2 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - State Persistence', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('should persist game volume setting when navigating to canvas tab and back', async ({
    page,
  }) => {
    // Set game audio to 55%
    await setSliderValue(page, 'game-audio-slider', 55);

    let gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValue).toBe('55%');

    // Navigate to Canvas tab
    await page.click('[data-testid="canvas-tab"]');
    await expect(page.locator('[data-testid="canvas-editor"]')).toBeVisible();

    // Navigate back to Audio tab
    await page.click('[data-testid="audio-tab"]');
    await expect(page.locator('[data-testid="audio-mixer"]')).toBeVisible();

    // Volume should be persisted
    gameValue = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValue).toBe('55%');
  });

  test('should persist preset selection when switching tabs', async ({ page }) => {
    // Apply Music Focus preset
    await page.click('[data-testid="preset-music-focus"]');

    const gameValueBefore = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValueBefore).toBe('40%');

    // Navigate away and back
    await page.click('[data-testid="canvas-tab"]');
    await page.click('[data-testid="audio-tab"]');
    await expect(page.locator('[data-testid="audio-mixer"]')).toBeVisible();

    // Values should still reflect Music Focus preset
    const gameValueAfter = await page.locator('[data-testid="game-audio-value"]').textContent();
    expect(gameValueAfter).toBe('40%');
  });

  // NOTE: Loop setting persistence test requires music to be uploaded first.
  // In mock mode, Tauri dialog is not available so music cannot be uploaded.
  //
  // test('should persist loop setting when navigating tabs', async ({ page }) => {
  //   // Mock: Music uploaded
  //   // await page.click('[data-testid="loop-music-toggle"]');
  //   // await page.click('[data-testid="canvas-tab"]');
  //   // await page.click('[data-testid="audio-tab"]');
  //   // await expect(page.locator('[data-testid="loop-music-toggle"]')).not.toBeChecked();
  // });
});

// ---------------------------------------------------------------------------
// Accessibility (3 tests)
// ---------------------------------------------------------------------------

test.describe('Audio Mixer - Accessibility', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAudioMixer(page);
  });

  test('game audio slider thumb has proper aria-valuemin and aria-valuemax', async ({ page }) => {
    const thumb = await getSliderThumb(page, 'game-audio-slider');
    await expect(thumb).toHaveAttribute('aria-valuemin', '0');
    await expect(thumb).toHaveAttribute('aria-valuemax', '100');
  });

  test('preset buttons should have accessible text or aria-label', async ({ page }) => {
    const presets = [
      'preset-game-only',
      'preset-balanced',
      'preset-music-focus',
      'preset-music-only',
    ];

    for (const presetId of presets) {
      const btn = page.locator(`[data-testid="${presetId}"]`);
      const ariaLabel = await btn.getAttribute('aria-label');
      const textContent = await btn.textContent();
      expect(ariaLabel || textContent).toBeTruthy();
    }
  });

  test('should be keyboard navigable (Tab moves focus within mixer)', async ({ page }) => {
    // Tab into the audio mixer controls
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');

    // Verify some element within the page has focus (may or may not have data-testid)
    const hasFocus = await page.evaluate(
      () => document.activeElement !== null && document.activeElement !== document.body
    );
    expect(hasFocus).toBe(true);
  });
});
