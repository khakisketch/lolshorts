import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SignupForm } from "./SignupForm";

const mockSignup = jest.fn();
const mockClearError = jest.fn();

jest.mock("@/lib/auth", () => ({
  useAuthStore: () => ({
    signup: mockSignup,
    isLoading: false,
    error: null,
    clearError: mockClearError,
  }),
}));

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: { email?: string }) =>
      params?.email ? `${key}:${params.email}` : key,
  }),
}));

describe("SignupForm desktop auth contract", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("shows the confirmation handoff without closing the dialog", async () => {
    mockSignup.mockResolvedValue("confirmation_required");
    const onSuccess = jest.fn();
    const onSwitchToLogin = jest.fn();

    render(
      <SignupForm onSuccess={onSuccess} onSwitchToLogin={onSwitchToLogin} />,
    );

    fireEvent.change(screen.getByTestId("signup-email-input"), {
      target: { value: "new@example.com" },
    });
    fireEvent.change(screen.getByTestId("signup-password-input"), {
      target: { value: "password123" },
    });
    fireEvent.change(screen.getByTestId("confirm-password-input"), {
      target: { value: "password123" },
    });
    fireEvent.click(screen.getByTestId("sign-up-button"));

    await waitFor(() =>
      expect(screen.getByTestId("signup-confirmation-required")).toBeVisible(),
    );
    expect(screen.getByRole("status")).toHaveTextContent("new@example.com");
    expect(onSuccess).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("signup-confirmation-login"));
    expect(onSwitchToLogin).toHaveBeenCalledTimes(1);
  });

  it("does not expose the unsupported browser OAuth control", () => {
    render(<SignupForm />);

    expect(
      screen.queryByTestId("google-signup-button"),
    ).not.toBeInTheDocument();
  });
});
