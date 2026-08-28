import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AutoEditResult } from "./AutoEditResult";
import { videoApi } from "@/api/video";

const navigate = jest.fn();

jest.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
jest.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }));
jest.mock("@/stores/autoEditStore", () => ({
  useAutoEditStore: () => ({ currentTemplate: null, backgroundMusic: null }),
}));
jest.mock("@/components/results/ShareDialog", () => ({
  ShareDialog: ({
    open,
    videoPath,
    resultId,
  }: {
    open: boolean;
    videoPath?: string;
    resultId?: string;
  }) =>
    open ? (
      <div
        data-testid="share-dialog"
        data-video-path={videoPath}
        data-result-id={resultId}
      >
        share dialog
      </div>
    ) : null,
}));
jest.mock("@/api/video", () => ({
  videoApi: {
    revalidateAutoEditResult: jest.fn().mockResolvedValue({
      status: "valid",
      issues: [],
    }),
  },
}));

const result = {
  job_id: "auto-edit-job-42",
  output_path: "C:/LoLShorts/exports/exact-result.mp4",
  duration: 60,
  clips_used: 3,
  file_size_bytes: 1024,
};

describe("AutoEditResult sharing", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("opens an in-place share dialog for the validated completed result", async () => {
    render(
      <AutoEditResult
        result={result}
        onStartNew={jest.fn()}
        onRegenerate={jest.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId("share-result-button"));

    await waitFor(() => {
      expect(screen.getByTestId("share-dialog")).toHaveAttribute(
        "data-video-path",
        result.output_path,
      );
      expect(screen.getByTestId("share-dialog")).toHaveAttribute(
        "data-result-id",
        result.job_id,
      );
    });
    expect(navigate).not.toHaveBeenCalled();
  });

  it("blocks sharing when legacy revalidation remains unknown", async () => {
    (videoApi.revalidateAutoEditResult as jest.Mock).mockResolvedValueOnce({
      status: "unknown",
      issues: [],
    });
    const alertSpy = jest.spyOn(window, "alert").mockImplementation(() => {});
    render(
      <AutoEditResult
        result={result}
        onStartNew={jest.fn()}
        onRegenerate={jest.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId("share-result-button"));

    await waitFor(() =>
      expect(alertSpy).toHaveBeenCalledWith("outputValidation.unknown"),
    );
    expect(screen.queryByTestId("share-dialog")).not.toBeInTheDocument();
  });
});
