import { act, render, screen, fireEvent } from "@testing-library/react";
import { PaymentModal } from "./PaymentModal";

// Mock shell plugin first
jest.mock("@tauri-apps/plugin-shell", () => ({
  open: jest.fn(),
}));

// Mock Tauri invoke after shell plugin
const mockInvoke = jest.fn();
global.window.__TAURI__ = {
  invoke: mockInvoke,
};

describe("PaymentModal", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  const flushPaymentStatus = async () => {
    await act(async () => {
      await Promise.resolve();
    });
  };

  it("renders payment modal when open", async () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);
    await flushPaymentStatus();

    // i18n mock returns keys, so we look for the i18n keys
    expect(screen.getByText("payment.upgradeToPro")).toBeInTheDocument();
    expect(screen.getByText("payment.features.title")).toBeInTheDocument();
  });

  it("does not render when closed", async () => {
    const { container } = render(
      <PaymentModal isOpen={false} onClose={() => {}} />,
    );
    await flushPaymentStatus();

    // Modal should not be visible
    expect(container.querySelector('[role="dialog"]')).not.toBeInTheDocument();
  });

  it("calls onClose when close button is clicked", async () => {
    const handleClose = jest.fn();
    render(<PaymentModal isOpen={true} onClose={handleClose} />);
    await flushPaymentStatus();

    // Find and click close button (Cancel button - i18n key)
    const closeButton = screen.getByText("common.cancel");
    fireEvent.click(closeButton);

    expect(handleClose).toHaveBeenCalled();
  });

  it("displays monthly and yearly pricing options", async () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);
    await flushPaymentStatus();

    expect(screen.getByText("payment.monthlyPlan")).toBeInTheDocument();
    expect(screen.getByText("payment.yearlyPlan")).toBeInTheDocument();
  });

  it("displays correct pricing amounts with Korean Won", async () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);
    await flushPaymentStatus();

    // i18n mock returns the key only, so we check for the i18n keys
    // aria-label uses interpolation which the mock doesn't handle the same way
    expect(
      screen.getAllByText("payment.monthlyPrice").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("payment.yearlyPrice").length,
    ).toBeGreaterThanOrEqual(1);

    // Check for strikethrough original price (hardcoded, not i18n)
    expect(screen.getByText("₩118,800")).toBeInTheDocument();
  });

  it("shows select plan button", async () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);
    await flushPaymentStatus();

    expect(screen.getByText("payment.selectPlan")).toBeInTheDocument();
  });

  it("keeps checkout disabled while payment is deferred", async () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);
    await flushPaymentStatus();

    expect(
      screen.getByText(/Payment and PRO upgrades are deferred/i),
    ).toBeInTheDocument();
    expect(screen.getByText("payment.selectPlan")).toBeDisabled();
    expect(
      screen.getByText(/Payment checkout is intentionally unavailable/i),
    ).toBeInTheDocument();
  });
});
