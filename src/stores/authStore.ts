// Re-export from auth.ts for test compatibility
export { useAuthStore } from "../lib/auth";

// Import types for interface definition
import type { User } from "../lib/auth";

// Re-export AuthState interface if needed
export interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
}
