export interface ConfigData {
  // Server settings
  ip: string;
  port: number;

  // App settings
  check_update: boolean;
  auto_update: boolean;
  no_fs?: boolean;
  log_to_file?: boolean;

  // Network settings
  password: string;
  admin_password: string;
  proxy: string | null;
  rproxy: string | null;

  // API settings
  max_retries: number;
  preserve_chats: boolean;
  web_search: boolean;
  enable_web_count_tokens: boolean;
  sanitize_messages: boolean;
  auto_probe_1m_sonnet: boolean;
  auto_probe_1m_opus_46: boolean;

  // Cookie settings
  skip_first_warning: boolean;
  skip_second_warning: boolean;
  skip_restricted: boolean;
  skip_non_pro: boolean;
  skip_rate_limit: boolean;
  skip_normal_pro: boolean;

  // Prompt configurations
  use_real_roles: boolean;
  custom_h: string | null;
  custom_a: string | null;
  custom_prompt: string;
  claude_code_client_id?: string | null;
  custom_system?: string | null;
}

export interface ConfigState {
  config: ConfigData | null;
  originalPassword: string;
  originalAdminPassword: string;
  loading: boolean;
  saving: boolean;
  error: string;
}
