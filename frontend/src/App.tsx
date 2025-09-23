// frontend/src/App.tsx
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import "./App.css";
import MainLayout from "./components/layout/MainLayout";
import Card from "./components/common/Card";
import TabNavigation from "./components/common/TabNavigation";
import AuthGatekeeper from "./components/auth/AuthGatekeeper";
import LogoutPanel from "./components/auth/LogoutPanel";
import ClaudeTabs from "./components/claude";
import ConfigTab from "./components/config";
import GeminiTab from "./components/gemini";
import StatusMessage from "./components/common/StatusMessage";
import ErrorBoundary from "./components/common/ErrorBoundary";
import { useAppContext } from "./context/AppContext";

function App() {
  const { t } = useTranslation();
  const {
    version,
    isAuthenticated,
    setIsAuthenticated,
    activeTab,
    setActiveTab,
  } = useAppContext();

  const [passwordChanged, setPasswordChanged] = useState(false);

  useEffect(() => {
    // Check if redirected due to password change
    const params = new URLSearchParams(window.location.search);
    if (params.get("passwordChanged") === "true") {
      setPasswordChanged(true);
      // Clean up the URL
      window.history.replaceState({}, document.title, window.location.pathname);
    }
  }, []);

  // Function to handle successful authentication
  const handleAuthenticated = (status: boolean) => {
    setIsAuthenticated(status);
  };

  // Function to handle logout
  const handleLogout = () => {
    localStorage.removeItem("authToken");
    setIsAuthenticated(false);
  };

  // Define tabs for the authenticated user
  const tabs = [
    { id: "claude", label: t("tabs.claude"), color: "cyan" },
    { id: "gemini", label: t("tabs.gemini"), color: "purple" },
    { id: "config", label: t("tabs.config"), color: "green" },
    { id: "auth", label: t("tabs.auth"), color: "violet" },
  ];

  return (
    <ErrorBoundary>
      <MainLayout version={version}>
        {isAuthenticated ? (
          // Protected content - only shown when authenticated
          <Card className="w-full max-w-md sm:max-w-lg md:max-w-xl mx-auto">
            <TabNavigation
              tabs={tabs}
              activeTab={activeTab}
              onTabChange={(tabId) => setActiveTab(tabId)}
              className="mb-6"
            />

            <ErrorBoundary>
              {activeTab === "claude" ? (
                <ClaudeTabs />
              ) : activeTab === "gemini" ? (
                <GeminiTab />
              ) : activeTab === "config" ? (
                <ConfigTab />
              ) : (
                <LogoutPanel onLogout={handleLogout} />
              )}
            </ErrorBoundary>
          </Card>
        ) : (
          // Auth gatekeeper - shown when not authenticated
          <Card className="w-full max-w-md sm:max-w-lg md:max-w-xl mx-auto">
            <h2 className="text-xl font-semibold text-center mb-6">
              {t("auth.title")}
            </h2>

            {passwordChanged && (
              <StatusMessage type="info" message={t("auth.passwordChanged")} />
            )}

            <p className="text-gray-400 text-sm mb-6 text-center">
              {t("auth.description")}
            </p>

            <ErrorBoundary>
              <AuthGatekeeper onAuthenticated={handleAuthenticated} />
            </ErrorBoundary>
          </Card>
        )}
      </MainLayout>
    </ErrorBoundary>
  );
}

export default App;
