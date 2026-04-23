export interface ScopeDescription {
  key: string;
  description: string;
  name: string;
  parent?: string;
}
export interface SmallIdentification {
  name: string;
}
export interface UserResponseType {
  id: number;
  name: string;
  username: string;
  email: string;
  active: boolean;
  require_password_change: boolean;
  admin: boolean;
  user_manager: boolean;
  system_manager: boolean;
  default_repository_actions: Array<RepositoryActions>;
  created_at: string;
}

export interface PublicUser {
  id: number;
  name: string;
  username: string;
  admin: boolean;
}

export interface Session {
  user_id: number;
  session_id: string;
  expires: Date;
  created: Date;
}

export interface Me {
  user: UserResponseType;
  session: Session;
}

export interface NewUser {
  username: string;
  password: string;
  email: string;
  name: string;
}
export interface SiteInfo {
  url?: string;
  mode: string;
  name: string;
  description: string;
  is_installed: boolean;
  version: string;
  password_rules?: PasswordRules;
  sso?: SsoInfo;
  oauth2?: InstanceOAuth2Settings;
}
export interface PasswordRules {
  min_length: number;
  require_uppercase: boolean;
  require_lowercase: boolean;
  require_number: boolean;
  require_symbol: boolean;
  require_special?: boolean;
}

export interface SsoInfo {
  login_path: string;
  login_button_text: string;
  provider_login_url?: string | null;
  provider_redirect_param?: string | null;
  auto_create_users: boolean;
}

export interface SsoConfiguration extends SsoInfo {
  enabled: boolean;
  providers: OidcProviderConfig[];
  role_claims: string[];
}

export type TokenSource =
  | { kind: "header"; name: string; prefix?: string | null }
  | { kind: "cookie"; name: string };

export interface OidcProviderConfig {
  name: string;
  issuer: string;
  audience: string;
  jwks_url?: string | null;
  token_source: TokenSource;
  subject_claim?: string | null;
  email_claim?: string | null;
  display_name_claim?: string | null;
  role_claims: string[];
}

export type OAuth2ProviderKind = "google" | "microsoft";

export interface OAuth2ProviderSummary {
  client_id: string;
  redirect_path?: string | null;
  tenant_id?: string | null;
  scopes: string[];
  client_secret_configured: boolean;
}

export interface OAuth2CasbinConfig {
  model: string;
  policy: string;
}

export interface OAuth2GroupRoleMapping {
  provider: OAuth2ProviderKind;
  group: string;
  roles: string[];
}

export interface InstanceOAuth2Provider {
  provider: string;
  redirect_path?: string | null;
}

export interface InstanceOAuth2Settings {
  login_path: string;
  callback_path: string;
  providers: InstanceOAuth2Provider[];
  auto_create_users: boolean;
  group_role_mappings: OAuth2GroupRoleMapping[];
}

export interface OAuth2Configuration {
  enabled: boolean;
  login_path: string;
  callback_path: string;
  redirect_base_url?: string | null;
  auto_create_users: boolean;
  google?: OAuth2ProviderSummary | null;
  microsoft?: OAuth2ProviderSummary | null;
  casbin?: OAuth2CasbinConfig | null;
  group_role_mappings: OAuth2GroupRoleMapping[];
  available_roles: string[];
}

export type WebhookEventType = "package.published" | "package.deleted";

export type WebhookDeliveryStatus = "pending" | "processing" | "delivered" | "failed";

export interface WebhookHeader {
  name: string;
  configured: boolean;
}

export interface WebhookConfiguration {
  id: string;
  name: string;
  enabled: boolean;
  target_url: string;
  events: WebhookEventType[];
  headers: WebhookHeader[];
  last_delivery_status?: WebhookDeliveryStatus | null;
  last_delivery_at?: string | null;
  last_http_status?: number | null;
  last_error?: string | null;
}

export interface WebhookHeaderUpdate {
  name: string;
  value?: string | null;
  configured: boolean;
}

export interface WebhookUpdatePayload {
  name: string;
  enabled: boolean;
  target_url: string;
  events: WebhookEventType[];
  headers: WebhookHeaderUpdate[];
}

export enum RepositoryActions {
  Read = "Read",
  Write = "Write",
  Edit = "Edit",
}
export function formatDate(date: Date) {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}
