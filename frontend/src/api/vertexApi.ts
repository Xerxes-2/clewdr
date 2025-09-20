import { VertexCredentialInfo } from "../types/vertex.types";

export async function postVertexCredential(credential: string) {
  const token = localStorage.getItem("authToken") || "";
  const response = await fetch("/api/vertex/credential", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ credential }),
  });

  if (response.status === 400) {
    throw new Error("Invalid credential JSON");
  }
  if (response.status === 401) {
    throw new Error("Authentication failed. Please set a valid auth token.");
  }
  if (response.status === 503) {
    throw new Error("Database storage is unavailable");
  }
  if (!response.ok) {
    const message = await response.text();
    throw new Error(message || `Error ${response.status}`);
  }
  return response;
}

export async function getVertexCredentials(): Promise<VertexCredentialInfo> {
  const token = localStorage.getItem("authToken") || "";
  const response = await fetch("/api/vertex/credentials", {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
  });
  if (!response.ok) {
    const message = await response.text();
    throw new Error(message || `Error ${response.status}`);
  }
  return response.json();
}

export async function deleteVertexCredential(id: string) {
  const token = localStorage.getItem("authToken") || "";
  const response = await fetch("/api/vertex/credential", {
    method: "DELETE",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ id }),
  });
  if (!response.ok) {
    const message = await response.text();
    throw new Error(message || `Error ${response.status}`);
  }
  return response;
}
