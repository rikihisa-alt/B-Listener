import { HashRouter, Navigate, Route, Routes } from "react-router-dom";

import { AppLayout } from "@/components/layout/AppLayout";
import { HomePage } from "@/features/home/HomePage";
import { MeetingListPage } from "@/features/home/MeetingListPage";
import { MeetingDetailPage } from "@/features/meeting-detail/MeetingDetailPage";
import { LiveMeetingPage } from "@/features/meeting-live/LiveMeetingPage";
import { NewMeetingPage } from "@/features/meeting-new/NewMeetingPage";
import { SettingsPage } from "@/features/settings/SettingsPage";

export function App() {
  return (
    <HashRouter>
      <AppLayout>
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/meetings" element={<MeetingListPage />} />
          <Route path="/meetings/new" element={<NewMeetingPage />} />
          <Route path="/meetings/:meetingId" element={<MeetingDetailPage />} />
          <Route path="/meetings/:meetingId/live" element={<LiveMeetingPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </AppLayout>
    </HashRouter>
  );
}
