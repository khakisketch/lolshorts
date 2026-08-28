/**
 * Auth Store Tests
 *
 * Tests for authentication state management and user session handling
 * Comprehensive coverage of auth functionality including login, logout, and error handling
 */

// Unmock auth and errorMapper modules to test the real implementation
jest.unmock("./auth");
jest.unmock("./errorMapper");

import { renderHook, act } from "@testing-library/react";
import { useAuthStore } from "./auth";
import { authApi } from "@/api/auth";

// Mock authApi to avoid backend calls
jest.mock("@/api/auth", () => ({
  authApi: {
    setSession: jest.fn().mockResolvedValue({
      user: {
        id: "test-user-id",
        email: "test@example.com",
        tier: "Free",
        expires_at: 9999999999,
      },
      entitlement: {
        tier: "FREE",
        status: "active",
        expires_at: null,
        source: "supabase",
        checked_at: "2026-01-01T00:00:00Z",
        payment_available: false,
      },
    }),
    getCurrentEntitlement: jest.fn().mockResolvedValue({
      tier: "FREE",
      status: "active",
      expires_at: null,
      source: "supabase",
      checked_at: "2026-01-01T00:00:00Z",
      payment_available: false,
    }),
    logout: jest.fn().mockResolvedValue(undefined),
  },
}));

// Mock Supabase
jest.mock("./supabase", () => ({
  supabase: {
    auth: {
      signInWithPassword: jest.fn(),
      signUp: jest.fn(),
      signOut: jest.fn(),
      refreshSession: jest.fn(),
      getSession: jest.fn(),
    },
    from: jest.fn(),
  },
}));

// Mock localStorage
const localStorageMock = {
  getItem: jest.fn(),
  setItem: jest.fn(),
  removeItem: jest.fn(),
  clear: jest.fn(),
};
Object.defineProperty(window, "localStorage", {
  value: localStorageMock,
});

describe("Auth Store", () => {
  beforeEach(() => {
    // Reset mocks
    jest.clearAllMocks();
    localStorageMock.getItem.mockReturnValue(null);

    // Reset store
    useAuthStore.setState({
      user: null,
      entitlement: null,
      isAuthenticated: false,
      isLoading: false,
      error: null,
    });
  });

  describe("Initial State", () => {
    it("should have correct initial state", () => {
      const { result } = renderHook(() => useAuthStore());

      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBeNull();
    });
  });

  describe("Login Functionality", () => {
    it("should handle successful login", async () => {
      const mockUser = {
        id: "test-user-id",
        email: "test@example.com",
        tier: "FREE" as const,
        profile: {
          id: "test-user-id",
          email: "test@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "test-user-id", email: "test@example.com" },
      };

      const { supabase } = require("./supabase");
      supabase.auth.signInWithPassword.mockResolvedValue({
        data: {
          user: mockUser.supabaseUser,
          session: {
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_at: 9999999999,
            user: mockUser.supabaseUser,
          },
        },
        error: null,
      });
      supabase.from.mockReturnValue({
        select: jest.fn().mockReturnValue({
          eq: jest.fn().mockReturnValue({
            single: jest.fn().mockResolvedValue({
              data: {
                id: "test-user-id",
                email: "test@example.com",
                display_name: null,
                avatar_url: null,
              },
              error: null,
            }),
          }),
        }),
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.login({
          email: "test@example.com",
          password: "password123",
        });
      });

      expect(result.current.user).toEqual(mockUser);
      expect(result.current.isAuthenticated).toBe(true);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBeNull();
    });

    it("should handle login error", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.signInWithPassword.mockResolvedValue({
        data: { user: null },
        error: { message: "Invalid credentials", code: "invalid_credentials" },
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await expect(
          result.current.login({
            email: "test@example.com",
            password: "wrong-password",
          }),
        ).rejects.toThrow("errors.invalidCredentials");
      });

      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBe("errors.invalidCredentials");
    });

    it("should handle network error during login", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.signInWithPassword.mockRejectedValue(
        new Error("Network error"),
      );

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await expect(
          result.current.login({
            email: "test@example.com",
            password: "password123",
          }),
        ).rejects.toThrow("errors.networkError");
      });

      expect(result.current.error).toBe("errors.networkError");
      expect(result.current.isLoading).toBe(false);
    });
  });

  describe("Signup Functionality", () => {
    it("should handle successful signup", async () => {
      const mockUser = {
        id: "new-user-id",
        email: "newuser@example.com",
        tier: "FREE" as const,
        profile: {
          id: "new-user-id",
          email: "newuser@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "new-user-id", email: "newuser@example.com" },
      };

      const { supabase } = require("./supabase");
      supabase.auth.signUp.mockResolvedValue({
        data: {
          user: mockUser.supabaseUser,
          session: {
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_at: 9999999999,
            user: mockUser.supabaseUser,
          },
        },
        error: null,
      });

      // Mock for insert operation
      supabase.from.mockReturnValueOnce({
        insert: jest.fn().mockResolvedValue({
          data: null,
          error: null,
        }),
      });

      // Mock for select operation
      supabase.from.mockReturnValueOnce({
        select: jest.fn().mockReturnValue({
          eq: jest.fn().mockReturnValue({
            single: jest.fn().mockResolvedValue({
              data: {
                id: "new-user-id",
                email: "newuser@example.com",
                display_name: null,
                avatar_url: null,
              },
              error: null,
            }),
          }),
        }),
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        const signupResult = await result.current.signup({
          email: "newuser@example.com",
          password: "password123",
          confirm_password: "password123",
        });
        expect(signupResult).toBe("signed_in");
      });

      expect(result.current.user).toEqual({
        ...mockUser,
        tier: "FREE", // Default tier for new users
      });
      expect(result.current.isAuthenticated).toBe(true);
    });

    it("should treat email confirmation as a successful pending signup", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.signUp.mockResolvedValue({
        data: {
          user: { id: "pending-user-id", email: "pending@example.com" },
          session: null,
        },
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      let signupResult: string | undefined;
      await act(async () => {
        signupResult = await result.current.signup({
          email: "pending@example.com",
          password: "password123",
          confirm_password: "password123",
        });
      });

      expect(signupResult).toBe("confirmation_required");
      expect(supabase.from).not.toHaveBeenCalled();
      expect(authApi.setSession).not.toHaveBeenCalled();
      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBeNull();
    });

    it("should reject signup with mismatched passwords", async () => {
      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await expect(
          result.current.signup({
            email: "test@example.com",
            password: "password123",
            confirm_password: "differentpassword",
          }),
        ).rejects.toThrow("errors.passwordsDoNotMatch");
      });

      expect(result.current.error).toBe("errors.passwordsDoNotMatch");
    });
  });

  describe("Logout Functionality", () => {
    it("should handle successful logout", async () => {
      // First set up authenticated state
      const mockUser = {
        id: "test-user-id",
        email: "test@example.com",
        tier: "PRO" as const,
        profile: {
          id: "test-user-id",
          email: "test@example.com",
          tier: "PRO",
        },
        supabaseUser: { id: "test-user-id", email: "test@example.com" },
      };

      const { supabase } = require("./supabase");
      supabase.auth.signOut.mockResolvedValue({ error: null });

      // Set authenticated state
      useAuthStore.setState({
        user: mockUser,
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.logout();
      });

      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.error).toBeNull();
      expect(authApi.logout).toHaveBeenCalledTimes(1);
    });

    it("should fail closed when a remote sign-out cannot reach the backend", async () => {
      (authApi.logout as jest.Mock).mockRejectedValueOnce(
        new Error("backend unavailable"),
      );
      useAuthStore.setState({
        user: {
          id: "remote-user-id",
          email: "remote@example.com",
          tier: "FREE",
          profile: null,
          supabaseUser: {
            id: "remote-user-id",
            email: "remote@example.com",
          } as never,
        },
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.syncSignedOut();
      });

      expect(result.current.user).toBeNull();
      expect(result.current.entitlement).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.error).toBeNull();
    });

    it("should handle logout error", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.signOut.mockRejectedValue(new Error("Logout failed"));

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await expect(result.current.logout()).rejects.toThrow("errors.generic");
      });

      expect(result.current.error).toBe("errors.generic");
    });
  });

  describe("Session Check", () => {
    it("should check and restore existing session", async () => {
      const mockUser = {
        id: "existing-user-id",
        email: "existing@example.com",
        tier: "PRO" as const,
        profile: {
          id: "existing-user-id",
          email: "existing@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "existing-user-id", email: "existing@example.com" },
      };

      const { supabase } = require("./supabase");
      (authApi.setSession as jest.Mock).mockResolvedValueOnce({
        user: {
          id: "existing-user-id",
          email: "existing@example.com",
          tier: "Pro",
          expires_at: 9999999999,
        },
        entitlement: {
          tier: "PRO",
          status: "active",
          expires_at: null,
          source: "supabase",
          checked_at: "2026-01-01T00:00:00Z",
          payment_available: false,
        },
      });
      supabase.auth.getSession.mockResolvedValue({
        data: {
          session: {
            user: mockUser.supabaseUser,
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_at: 9999999999,
          },
        },
        error: null,
      });

      supabase.from.mockReturnValue({
        select: jest.fn().mockReturnValue({
          eq: jest.fn().mockReturnValue({
            single: jest.fn().mockResolvedValue({
              data: {
                id: "existing-user-id",
                email: "existing@example.com",
                display_name: null,
                avatar_url: null,
              },
              error: null,
            }),
          }),
        }),
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.checkAuth();
      });

      expect(result.current.user).toEqual(mockUser);
      expect(result.current.isAuthenticated).toBe(true);
    });

    it("should handle no existing session", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.getSession.mockResolvedValue({
        data: { session: null },
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.checkAuth();
      });

      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.isLoading).toBe(false);
    });
  });

  describe("License Info", () => {
    it("should return license info for PRO user", async () => {
      const proUser = {
        id: "pro-user-id",
        email: "pro@example.com",
        tier: "PRO" as const,
        profile: {
          id: "pro-user-id",
          email: "pro@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "pro-user-id", email: "pro@example.com" },
      };
      (authApi.getCurrentEntitlement as jest.Mock).mockResolvedValueOnce({
        tier: "PRO",
        status: "active",
        expires_at: "2024-12-31T23:59:59Z",
        source: "supabase",
        checked_at: "2026-01-01T00:00:00Z",
        payment_available: false,
      });

      useAuthStore.setState({
        user: proUser,
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      let licenseInfo;
      await act(async () => {
        licenseInfo = await result.current.getLicenseInfo();
      });

      expect(licenseInfo).toEqual({
        tier: "PRO",
        expires_at: "2024-12-31T23:59:59Z",
        features: [
          "unlimited_clips",
          "advanced_editor",
          "priority_support",
          "no_watermarks",
        ],
      });
    });

    it("should return license info for FREE user", async () => {
      const freeUser = {
        id: "free-user-id",
        email: "free@example.com",
        tier: "FREE" as const,
        profile: {
          id: "free-user-id",
          email: "free@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "free-user-id", email: "free@example.com" },
      };
      (authApi.getCurrentEntitlement as jest.Mock).mockResolvedValueOnce({
        tier: "FREE",
        status: "active",
        expires_at: null,
        source: "supabase",
        checked_at: "2026-01-01T00:00:00Z",
        payment_available: false,
      });

      useAuthStore.setState({
        user: freeUser,
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      let licenseInfo;
      await act(async () => {
        licenseInfo = await result.current.getLicenseInfo();
      });

      expect(licenseInfo).toEqual({
        tier: "FREE",
        features: ["basic_clips", "basic_editor"],
      });
    });

    it("should fail closed to FREE when entitlement refresh fails", async () => {
      const proUser = {
        id: "user-id",
        email: "user@example.com",
        tier: "PRO" as const,
        profile: null,
        supabaseUser: { id: "user-id", email: "user@example.com" },
      };
      (authApi.getCurrentEntitlement as jest.Mock).mockRejectedValueOnce(
        new Error("offline"),
      );

      useAuthStore.setState({
        user: proUser,
        entitlement: {
          tier: "PRO",
          status: "active",
          expires_at: null,
          source: "supabase",
          checked_at: "2026-01-01T00:00:00Z",
          payment_available: false,
        },
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      let licenseInfo;
      await act(async () => {
        licenseInfo = await result.current.getLicenseInfo();
      });
      expect(licenseInfo).toEqual({
        tier: "FREE",
        features: ["basic_clips", "basic_editor"],
      });
      expect(result.current.user?.tier).toBe("FREE");
      expect(result.current.entitlement?.tier).toBe("FREE");
    });
  });

  describe("Error Management", () => {
    it("should clear error state", () => {
      useAuthStore.setState({
        user: null,
        isAuthenticated: false,
        isLoading: false,
        error: "Previous error",
      });

      const { result } = renderHook(() => useAuthStore());

      act(() => {
        result.current.clearError();
      });

      expect(result.current.error).toBeNull();
    });
  });

  describe("Token Refresh", () => {
    it("should refresh token successfully", async () => {
      const mockUser = {
        id: "refresh-user-id",
        email: "refresh@example.com",
        tier: "PRO" as const,
        profile: {
          id: "refresh-user-id",
          email: "refresh@example.com",
          display_name: null,
          avatar_url: null,
        },
        supabaseUser: { id: "refresh-user-id", email: "refresh@example.com" },
      };

      const { supabase } = require("./supabase");
      (authApi.setSession as jest.Mock).mockResolvedValueOnce({
        user: {
          id: "refresh-user-id",
          email: "refresh@example.com",
          tier: "Pro",
          expires_at: 9999999999,
        },
        entitlement: {
          tier: "PRO",
          status: "active",
          expires_at: null,
          source: "supabase",
          checked_at: "2026-01-01T00:00:00Z",
          payment_available: false,
        },
      });
      supabase.auth.refreshSession.mockResolvedValue({
        data: {
          user: mockUser.supabaseUser,
          session: {
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_at: 9999999999,
            user: mockUser.supabaseUser,
          },
        },
        error: null,
      });

      // Mock for profile lookup during refresh
      supabase.from.mockReturnValueOnce({
        select: jest.fn().mockReturnValue({
          eq: jest.fn().mockReturnValue({
            single: jest.fn().mockResolvedValue({
              data: mockUser.profile,
              error: null,
            }),
          }),
        }),
      });

      useAuthStore.setState({
        user: mockUser,
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.refreshToken();
      });

      // Token refresh should not change the auth state if successful
      expect(authApi.setSession).toHaveBeenCalledWith(
        "access-token",
        "refresh-token",
        "refresh-user-id",
        "refresh@example.com",
        9999999999,
      );
      expect(result.current.user).toEqual(mockUser);
      expect(result.current.isAuthenticated).toBe(true);
    });

    it("should handle token refresh failure", async () => {
      const { supabase } = require("./supabase");
      supabase.auth.refreshSession.mockResolvedValue({
        data: { user: null },
        error: { message: "Token expired" },
      });

      useAuthStore.setState({
        user: {
          id: "user-id",
          email: "user@example.com",
          tier: "FREE" as const,
          profile: {
            id: "user-id",
            email: "user@example.com",
            tier: "FREE" as const,
          },
          supabaseUser: { id: "user-id", email: "user@example.com" },
        },
        isAuthenticated: true,
        isLoading: false,
        error: null,
      });

      const { result } = renderHook(() => useAuthStore());

      await act(async () => {
        await result.current.refreshToken();
      });

      expect(result.current.user).toBeNull();
      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.error).toBe("errors.sessionExpired");
    });
  });
});
