import {
  VertexCredentialInfo,
  VertexServiceAccount,
} from "../types/vertex.types";

function getToken() {
  return localStorage.getItem("authToken") || "";
}

export async function getVertexCredentials(): Promise<VertexCredentialInfo[]> {
  const response = await fetch("/api/vertex/credentials", {
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "application/json",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to load vertex credentials: ${response.status}`);
  }

  return (await response.json()) as VertexCredentialInfo[];
}

export async function addVertexCredential(credential: VertexServiceAccount) {
  const response = await fetch("/api/vertex/credential", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ credential }),
  });

  if (!response.ok) {
    throw new Error(`Failed to add vertex credential: ${response.status}`);
  }

  return response;
}

export async function deleteVertexCredential(clientEmail: string) {
  const response = await fetch("/api/vertex/credential", {
    method: "DELETE",
    headers: {
      Authorization: `Bearer ${getToken()}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ client_email: clientEmail }),
  });

  if (!response.ok) {
    throw new Error(`Failed to delete vertex credential: ${response.status}`);
  }

  return response;
}
