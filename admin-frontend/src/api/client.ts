/**
 * Typed API client for the django-rs admin backend.
 *
 * Handles authentication, request construction, and response parsing.
 * All methods return typed responses matching the Rust backend structs.
 */

import type {
  ModelIndexResponse,
  ModelSchemaResponse,
  JsonListResponse,
  ListParams,
  CurrentUserResponse,
  LoginRequest,
  LoginResponse,
  LogEntry,
  CreateObjectRequest,
  UpdateObjectRequest,
  BulkActionRequest,
  BulkActionResponse,
} from '../types/api';

const API_BASE = '/api/admin';

// ── Token Management ────────────────────────────────────────────────

const TOKEN_KEY = 'django_rs_admin_token';

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

// ── HTTP Helpers ────────────────────────────────────────────────────

class ApiClientError extends Error {
  status: number;
  statusText: string;
  body: unknown;

  constructor(status: number, statusText: string, body: unknown) {
    super(`API Error ${status}: ${statusText}`);
    this.name = 'ApiClientError';
    this.status = status;
    this.statusText = statusText;
    this.body = body;
  }
}

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = getToken();
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> | undefined),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const response = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers,
  });

  if (!response.ok) {
    let body: unknown;
    try {
      body = await response.json();
    } catch {
      body = await response.text();
    }
    throw new ApiClientError(response.status, response.statusText, body);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

function buildQueryString(params: ListParams): string {
  const searchParams = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && value !== '') {
      searchParams.set(key, String(value));
    }
  }
  const qs = searchParams.toString();
  return qs ? `?${qs}` : '';
}

// ── Authentication ──────────────────────────────────────────────────

export async function login(credentials: LoginRequest): Promise<LoginResponse> {
  const data = await request<LoginResponse>('/login/', {
    method: 'POST',
    body: JSON.stringify(credentials),
  });
  if (data.token) {
    setToken(data.token);
  }
  return data;
}

export async function logout(): Promise<void> {
  try {
    await request<void>('/logout/', { method: 'POST' });
  } finally {
    clearToken();
  }
}

export async function getCurrentUser(): Promise<CurrentUserResponse> {
  return request<CurrentUserResponse>('/me/');
}

// ── Model Index ─────────────────────────────────────────────────────

export async function getModelIndex(): Promise<ModelIndexResponse> {
  return request<ModelIndexResponse>('/');
}

// ── Model Schema ────────────────────────────────────────────────────

export async function getModelSchema(
  appLabel: string,
  modelName: string,
): Promise<ModelSchemaResponse> {
  return request<ModelSchemaResponse>(`/${appLabel}/${modelName}/schema`);
}

// ── CRUD Operations ─────────────────────────────────────────────────

export async function listObjects(
  appLabel: string,
  modelName: string,
  params: ListParams = {},
): Promise<JsonListResponse> {
  const qs = buildQueryString(params);
  return request<JsonListResponse>(`/${appLabel}/${modelName}/${qs}`);
}

export async function getObject(
  appLabel: string,
  modelName: string,
  pk: string,
): Promise<Record<string, unknown>> {
  return request<Record<string, unknown>>(`/${appLabel}/${modelName}/${pk}/`);
}

export async function createObject(
  appLabel: string,
  modelName: string,
  data: CreateObjectRequest,
): Promise<Record<string, unknown>> {
  return request<Record<string, unknown>>(`/${appLabel}/${modelName}/`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function updateObject(
  appLabel: string,
  modelName: string,
  pk: string,
  data: UpdateObjectRequest,
): Promise<Record<string, unknown>> {
  return request<Record<string, unknown>>(`/${appLabel}/${modelName}/${pk}/`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function deleteObject(
  appLabel: string,
  modelName: string,
  pk: string,
): Promise<void> {
  return request<void>(`/${appLabel}/${modelName}/${pk}/`, {
    method: 'DELETE',
  });
}

// ── Bulk Actions ────────────────────────────────────────────────────

export async function executeBulkAction(
  appLabel: string,
  modelName: string,
  actionData: BulkActionRequest,
): Promise<BulkActionResponse> {
  return request<BulkActionResponse>(`/${appLabel}/${modelName}/action/`, {
    method: 'POST',
    body: JSON.stringify(actionData),
  });
}

// ── LogEntry ────────────────────────────────────────────────────────

export async function getRecentActions(limit = 10): Promise<LogEntry[]> {
  return request<LogEntry[]>(`/log/?limit=${limit}`);
}

export async function getObjectHistory(
  contentType: string,
  objectId: string,
): Promise<LogEntry[]> {
  return request<LogEntry[]>(
    `/log/${contentType}/${objectId}/`,
  );
}

// ── Export ───────────────────────────────────────────────────────────

export { ApiClientError };
