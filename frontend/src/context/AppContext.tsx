// frontend/src/context/AppContext.tsx
import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  ReactNode,
} from "react";
import { getVersion } from "../api";

interface AppContextType {
  version: string;
  isAuthenticated: boolean;
  setIsAuthenticated: (status: boolean) => void;
  activeTab: string;
  setActiveTab: (tab: string) => void;
}

const defaultContext: AppContextType = {
  version: "",
  isAuthenticated: false,
  setIsAuthenticated: () => {},
  activeTab: "claude",
  setActiveTab: () => {},
};

const AppContext = createContext<AppContextType>(defaultContext);

interface AppProviderProps {
  children: ReactNode;
}

export const AppProvider: React.FC<AppProviderProps> = ({ children }) => {
  const [version, setVersion] = useState("");
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [activeTab, setActiveTab] = useState("claude");

  useEffect(() => {
    // Fetch and set the version when component mounts
    getVersion().then((v) => setVersion(v));

    // Check for authentication status
    const checkAuth = async () => {
      const storedToken = localStorage.getItem("authToken");
      if (storedToken) {
        try {
          const response = await fetch("/api/auth", {
            method: "GET",
            headers: {
              Authorization: `Bearer ${storedToken}`,
              "Content-Type": "application/json",
            },
          });

          if (response.ok) {
            setIsAuthenticated(true);
          } else {
            // Invalid token, clear it
            localStorage.removeItem("authToken");
            setIsAuthenticated(false);
          }
        } catch (error) {
          console.error("Authentication check failed:", error);
          setIsAuthenticated(false);
        }
      }
    };

    checkAuth();
  }, []);

  return (
    <AppContext.Provider
      value={{
        version,
        isAuthenticated,
        setIsAuthenticated,
        activeTab,
        setActiveTab,
      }}
    >
      {children}
    </AppContext.Provider>
  );
};

export const useAppContext = () => useContext(AppContext);
