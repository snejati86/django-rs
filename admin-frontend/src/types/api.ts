/**
 * TypeScript types matching the django-rs admin backend Rust structs.
 *
 * These types correspond to the serialized JSON shapes from:
 * - crates/django-rs-admin/src/api.rs
 * - crates/django-rs-admin/src/model_admin.rs
 * - crates/django-rs-admin/src/log_entry.rs
 * - crates/django-rs-admin/src/db.rs
 */

// ── Authentication ──────────────────────────────────────────────────

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: CurrentUserResponse;
}

export interface CurrentUserResponse {
  username: string;
  email: string;
  is_staff: boolean;
  is_superuser: boolean;
  full_name: string;
}

// ── Model Index ─────────────────────────────────────────────────────

export interface ModelIndexResponse {
  site_name: string;
  apps: AppModels[];
}

export interface AppModels {
  app_label: string;
  models: ModelInfo[];
}

export interface ModelInfo {
  name: string;
  verbose_name: string;
  verbose_name_plural: string;
  url: string;
}

// ── Model Schema ────────────────────────────────────────────────────

export interface ModelSchemaResponse {
  app_label: string;
  model_name: string;
  verbose_name: string;
  verbose_name_plural: string;
  fields: FieldSchema[];
  list_display: string[];
  search_fields: string[];
  ordering: string[];
  actions: string[];
  list_per_page: number;
}

export interface FieldSchema {
  name: string;
  field_type: string;
  required: boolean;
  read_only: boolean;
  primary_key: boolean;
  max_length: number | null;
  label: string;
  help_text: string;
  choices: [string, string][] | null;
  is_relation: boolean;
  related_model: string | null;
}

// ── List Response (Paginated) ───────────────────────────────────────

export interface JsonListResponse {
  results: Record<string, unknown>[];
  count: number;
  page: number;
  page_size: number;
  total_pages: number;
  has_next: boolean;
  has_previous: boolean;
}

export interface AdminListResult {
  response: JsonListResponse;
  filter_choices: Record<string, string[]>;
}

// ── List Parameters ─────────────────────────────────────────────────

export interface ListParams {
  page?: number;
  page_size?: number;
  search?: string;
  ordering?: string;
  [key: string]: string | number | undefined;
}

// ── LogEntry ────────────────────────────────────────────────────────

export type ActionFlag = 'Addition' | 'Change' | 'Deletion';

export interface LogEntry {
  id: number;
  action_time: string;
  user_id: number;
  content_type: string;
  object_id: string;
  object_repr: string;
  action_flag: ActionFlag;
  change_message: string;
}

// ── Mutation Payloads ───────────────────────────────────────────────

export interface CreateObjectRequest {
  [field: string]: unknown;
}

export interface UpdateObjectRequest {
  [field: string]: unknown;
}

export interface BulkActionRequest {
  action: string;
  ids: string[];
}

export interface BulkActionResponse {
  action: string;
  affected: number;
  message: string;
}

// ── API Error ───────────────────────────────────────────────────────

export interface ApiError {
  error: string;
  detail?: string;
}
