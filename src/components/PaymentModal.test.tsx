import { render, screen, fireEvent } from "@testing-library/react";
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

  it("renders payment modal when open", () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);

    // i18n mock returns keys, so we look for the i18n keys
    expect(screen.getByText("payment.upgradeToPro")).toBeInTheDocument();
    expect(screen.getByText("payment.features.title")).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { container } = render(
      <PaymentModal isOpen={false} onClose={() => {}} />,
    );

    // Modal should not be visible
    expect(container.querySelector('[role="dialog"]')).not.toBeInTheDocument();
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = jest.fn();
    render(<PaymentModal isOpen={true} onClose={handleClose} />);

    // Find and click close button (Cancel button - i18n key)
    const closeButton = screen.getByText("common.cancel");
    fireEvent.click(closeButton);

    expect(handleClose).toHaveBeenCalled();
  });

  it("displays monthly and yearly pricing options", () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);

    expect(screen.getByText("payment.monthlyPlan")).toBeInTheDocument();
    expect(screen.getByText("payment.yearlyPlan")).toBeInTheDocument();
  });

  it("displays correct pricing amounts with Korean Won", () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);

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

  it("shows select plan button", () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);

    expect(screen.getByText("payment.selectPlan")).toBeInTheDocument();
  });

  it("keeps checkout disabled while payment is deferred", () => {
    render(<PaymentModal isOpen={true} onClose={() => {}} />);

    expect(
      screen.getByText(/Payment and PRO upgrades are deferred/i),
    ).toBeInTheDocument();
    expect(screen.getByText("payment.selectPlan")).toBeDisabled();
    expect(
      screen.getByText(/Payment checkout is intentionally unavailable/i),
    ).toBeInTheDocument();
  });
});
