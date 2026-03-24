import { useEffect } from 'react';
import { BrowserRouter, Routes, Route, useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { Layout } from './components/Layout';
import { RecordingProvider } from './hooks/RecordingProvider';
import { ToastProvider } from './components/Toast';
import { RecordingPage } from './pages/RecordingPage';
import { SettingsPage } from './pages/SettingsPage';
import { MeetingsPage } from './pages/MeetingsPage';
import { MeetingDetailPage } from './pages/MeetingDetailPage';
import './global.scss';

/** Listens for tray-navigate events and navigates to the requested route. */
function TrayNavigator() {
  const navigate = useNavigate();

  useEffect(() => {
    const unlisten = listen<string>('tray-navigate', (event) => {
      navigate(event.payload);
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, [navigate]);

  return null;
}

function App() {
  return (
    <BrowserRouter>
      <TrayNavigator />
      <ToastProvider>
        <RecordingProvider>
          <Routes>
            <Route element={<Layout />}>
              <Route path="/" element={<RecordingPage />} />
              <Route path="/meetings" element={<MeetingsPage />} />
              <Route
                path="/meetings/:meetingId"
                element={<MeetingDetailPage />}
              />
              <Route path="/settings" element={<SettingsPage />} />
            </Route>
          </Routes>
        </RecordingProvider>
      </ToastProvider>
    </BrowserRouter>
  );
}

export default App;
