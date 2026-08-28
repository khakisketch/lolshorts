import { render, screen } from "@testing-library/react";
import { ProtectedFeature } from "./ProtectedFeature";
import { useAuthStore } from "@/lib/auth";

// Mock the auth store
jest.mock("@/lib/auth");

describe("ProtectedFeature", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("renders children when user is authenticated", () => {
    // Mock authenticated user
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { id: "123", email: "test@example.com" },
      isAuthenticated: true,
    });

    render(
      <ProtectedFeature>
        <div>Protected Content</div>
      </ProtectedFeature>,
    );

    expect(screen.getByText("Protected Content")).toBeInTheDocument();
  });

  it("renders non-PRO features for authenticated FREE users", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { id: "123", email: "test@example.com", tier: "FREE" },
      isAuthenticated: true,
    });

    render(
      <ProtectedFeature requiresPro={false} featureName="Auto-Edit">
        <div>Auto-Edit Content</div>
      </ProtectedFeature>,
    );

    expect(screen.getByText("Auto-Edit Content")).toBeInTheDocument();
  });

  it("blocks PRO-only features for authenticated FREE users", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { id: "123", email: "test@example.com", tier: "FREE" },
      isAuthenticated: true,
      entitlement: { tier: "FREE", status: "active" },
    });

    render(
      <ProtectedFeature requiresPro={true} featureName="YouTube">
        <div>YouTube Content</div>
      </ProtectedFeature>,
    );

    expect(screen.queryByText("YouTube Content")).not.toBeInTheDocument();
    expect(screen.getByText("PRO")).toBeInTheDocument();
  });

  it("does not trust a persisted user tier without active entitlement", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { id: "123", email: "test@example.com", tier: "PRO" },
      isAuthenticated: true,
      entitlement: { tier: "FREE", status: "active" },
    });

    render(
      <ProtectedFeature requiresPro={true} featureName="YouTube">
        <div>YouTube Content</div>
      </ProtectedFeature>,
    );

    expect(screen.queryByText("YouTube Content")).not.toBeInTheDocument();
    expect(screen.getByText("PRO")).toBeInTheDocument();
  });

  it("renders PRO-only features only with active Supabase entitlement", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { id: "123", email: "test@example.com", tier: "FREE" },
      isAuthenticated: true,
      entitlement: { tier: "PRO", status: "active" },
    });

    render(
      <ProtectedFeature requiresPro={true} featureName="YouTube">
        <div>YouTube Content</div>
      </ProtectedFeature>,
    );

    expect(screen.getByText("YouTube Content")).toBeInTheDocument();
  });

  it("renders fallback when user is not authenticated", () => {
    // Mock unauthenticated state
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: null,
      isAuthenticated: false,
    });

    render(
      <ProtectedFeature fallback={<div>Please login</div>}>
        <div>Protected Content</div>
      </ProtectedFeature>,
    );

    expect(screen.getByText("Please login")).toBeInTheDocument();
    expect(screen.queryByText("Protected Content")).not.toBeInTheDocument();
  });

  it("renders default fallback when no fallback is provided", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: null,
      isAuthenticated: false,
    });

    render(
      <ProtectedFeature>
        <div>Protected Content</div>
      </ProtectedFeature>,
    );

    // Should render nothing or default message
    expect(screen.queryByText("Protected Content")).not.toBeInTheDocument();
  });
});
