import { videoApi } from './video';
import { cmd } from './client';

jest.mock('./client', () => ({
  cmd: jest.fn(),
}));

const mockCmd = jest.mocked(cmd);

describe('videoApi AutoEdit normalization', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('normalizes backend AutoEdit result fields into the frontend contract', async () => {
    mockCmd.mockResolvedValueOnce({
      output_path: 'C:/clips/auto-edit.mp4',
      selected_clips: [{ id: 1 }, { id: 2 }],
      total_duration: 58.8,
      clip_count: 2,
    });

    await expect(
      videoApi.startAutoEdit({
        game_ids: ['game-1'],
        target_duration: 60,
      }),
    ).resolves.toEqual({
      job_id: '',
      output_path: 'C:/clips/auto-edit.mp4',
      duration: 58.8,
      clips_used: 2,
      file_size_bytes: 0,
    });

    expect(mockCmd).toHaveBeenCalledWith('start_auto_edit', {
      config: {
        game_ids: ['game-1'],
        target_duration: 60,
      },
    });
  });

  it('normalizes backend AutoEdit progress status and progress fields', async () => {
    mockCmd.mockResolvedValueOnce({
      job_id: 'auto_edit_20260425_120000',
      status: 'completed',
      progress: 100,
      current_step: 'Complete',
      estimated_seconds: 0,
      output_path: 'C:/clips/auto-edit.mp4',
    });

    await expect(videoApi.getAutoEditProgress()).resolves.toEqual({
      job_id: 'auto_edit_20260425_120000',
      status: 'Complete',
      progress_percentage: 100,
      current_stage: 'Complete',
      clips_selected: 0,
      total_clips: 0,
      estimated_completion_seconds: 0,
      output_path: 'C:/clips/auto-edit.mp4',
    });

    expect(mockCmd).toHaveBeenCalledWith('get_auto_edit_progress');
  });
});
