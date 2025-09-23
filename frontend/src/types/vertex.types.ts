export interface VertexCredentialStatus {
  id: string;
  client_email?: string | null;
  project_id?: string | null;
  count_403: number;
}

export interface VertexCredentialInfo {
  credentials: VertexCredentialStatus[];
}
