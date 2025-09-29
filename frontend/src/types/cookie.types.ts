// frontend/src/types/cookie.types.ts
export interface CookieStatus {
  cookie: string;
  reset_time: number | null;
  supports_claude_1m?: boolean | null;
  total_input_tokens?: number;
  total_output_tokens?: number;
  window_input_tokens?: number;
  window_output_tokens?: number;
}

export interface UselessCookie {
  cookie: string;
  reason: string | any;
}

export interface CookieStatusInfo {
  valid: CookieStatus[];
  exhausted: CookieStatus[];
  invalid: UselessCookie[];
}

export interface CookieFormState {
  cookie: string;
  isSubmitting: boolean;
  status: {
    type: "idle" | "success" | "error";
    message: string;
  };
}
