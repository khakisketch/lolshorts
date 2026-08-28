import { fireEvent, render, screen } from "@testing-library/react";
import { AutoEditStoryboard } from "./AutoEditStoryboard";

jest.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => path,
}));
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      typeof params?.defaultValue === "string" ? params.defaultValue : key,
  }),
}));

const clips = [
  {
    game_id: "game-a",
    file_path: "C:/clips/a.mp4",
    order: 0,
    trim_start_secs: 0,
    trim_end_secs: 100,
    source_duration_secs: 100,
    event_offset_secs: 30,
    event_type: "PentaKill",
    highlight_score: 100,
    recommended_order: 0,
    thumbnail_path: null,
  },
  {
    game_id: "game-b",
    file_path: "C:/clips/b.mp4",
    order: 1,
    trim_start_secs: 0,
    trim_end_secs: 95,
    source_duration_secs: 95,
    event_offset_secs: 40,
    event_type: "BaronKill",
    highlight_score: 70,
    recommended_order: 1,
    thumbnail_path: null,
  },
];

it("warns above 180 seconds, disables one Short, and keeps accessible reorder controls", () => {
  const onMove = jest.fn();
  const onOutputIntentChange = jest.fn();
  render(
    <AutoEditStoryboard
      clips={clips}
      outputIntent="shorts_series"
      onOutputIntentChange={onOutputIntentChange}
      onMove={onMove}
      onTrim={jest.fn()}
      onRemove={jest.fn()}
      onResetRecommendation={jest.fn()}
      onBack={jest.fn()}
      onGenerate={jest.fn()}
    />,
  );

  expect(
    screen.getByText("autoEdit.storyboard.over180Title"),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "autoEdit.output.single" }),
  ).toBeDisabled();
  fireEvent.click(
    screen.getAllByRole("button", { name: "autoEdit.storyboard.moveDown" })[0],
  );
  expect(onMove).toHaveBeenCalledWith(0, 1);
  expect(
    screen.getByRole("button", { name: "autoEdit.storyboard.generateSeries" }),
  ).toBeEnabled();
});
