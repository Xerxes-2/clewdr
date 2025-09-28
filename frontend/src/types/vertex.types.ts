export interface VertexCredentialInfo {
  client_email: string;
  project_id?: string | null;
  private_key_id?: string | null;
}

export interface VertexServiceAccount {
  client_email: string;
  project_id?: string;
  private_key_id?: string;
  private_key?: string;
  type?: string;
  client_id?: string;
  auth_uri?: string;
  token_uri?: string;
  auth_provider_x509_cert_url?: string;
  client_x509_cert_url?: string;
  universe_domain?: string;
  [key: string]: unknown;
}
