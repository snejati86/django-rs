import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from 'react';
import type { CurrentUserResponse } from '../types/api';
import * as api from '../api/client';

interface AuthState {
  user: CurrentUserResponse | null;
  isAuthenticated: boolean;
  isLoading: boolean;
}

interface AuthContextValue extends AuthState {
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

function getInitialState(): AuthState {
  const token = api.getToken();
  if (token) {
    // We have a token, so start in loading to verify it
    return { user: null, isAuthenticated: false, isLoading: true };
  }
  // No token - start as unauthenticated, not loading
  return { user: null, isAuthenticated: false, isLoading: false };
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>(getInitialState);

  // Verify stored token on mount (only runs the fetch, not synchronous setState)
  useEffect(() => {
    const token = api.getToken();
    if (!token) return;

    let cancelled = false;
    api
      .getCurrentUser()
      .then((user) => {
        if (!cancelled) {
          setState({ user, isAuthenticated: true, isLoading: false });
        }
      })
      .catch(() => {
        if (!cancelled) {
          api.clearToken();
          setState({ user: null, isAuthenticated: false, isLoading: false });
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const login = useCallback(async (username: string, password: string) => {
    const response = await api.login({ username, password });
    setState({
      user: response.user,
      isAuthenticated: true,
      isLoading: false,
    });
  }, []);

  const logout = useCallback(async () => {
    await api.logout();
    setState({ user: null, isAuthenticated: false, isLoading: false });
  }, []);

  return (
    <AuthContext.Provider
      value={{
        ...state,
        login,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
