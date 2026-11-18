import React, { useCallback, useEffect, useState } from "react";
import { commands, type Note, type Project } from "@/bindings";
import { useTranscriptionJobsStore } from "@/stores/transcriptionJobsStore";
import { NotesList, type NotesMode } from "./NotesList";
import { MeetingRecording } from "./MeetingRecording";
import { NoteView } from "./NoteView";
import { useMeetingStatus, useNotesChanged } from "./noteUtils";

type View =
  | { name: "list" }
  | { name: "recording" }
  | { name: "note"; id: number };

/** The Notes tab (ideas and written notes) or the Meetings tab, sorted
 * into projects (design/wireframes/NotesTab, MeetingsTab, NoteOpen). */
export const NotesPage: React.FC<{
  mode: NotesMode;
  onOpenPostProcess: () => void;
  onOpenSettings: () => void;
}> = ({ mode, onOpenPostProcess, onOpenSettings }) => {
  const [view, setView] = useState<View>({ name: "list" });
  const [notes, setNotes] = useState<Note[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loaded, setLoaded] = useState(false);
  const meeting = useMeetingStatus();
  const jobs = useTranscriptionJobsStore((s) => s.jobs);

  const reload = useCallback(async () => {
    const [n, p] = await Promise.all([
      commands.listNotes(null),
      commands.listProjects(),
    ]);
    if (n.status === "ok")
      setNotes(
        n.data.filter((note) =>
          mode === "meetings"
            ? note.kind === "meeting"
            : note.kind !== "meeting",
        ),
      );
    if (p.status === "ok") setProjects(p.data);
    setLoaded(true);
  }, [mode]);

  useEffect(() => {
    useTranscriptionJobsStore.getState().initialize();
    reload();
  }, [reload]);
  useNotesChanged(reload);

  // A note's text arrives when its job ends; reload to copy it in.
  const jobCount = Object.keys(jobs).length;
  useEffect(() => {
    if (loaded) reload();
  }, [jobCount]);

  // A meeting still recording (after leaving Meetings) opens its screen again.
  useEffect(() => {
    if (mode === "meetings" && meeting.recording && view.name === "list")
      setView({ name: "recording" });
  }, [meeting.recording]);

  const openNote = (id: number) => setView({ name: "note", id });
  const backToList = () => {
    setView({ name: "list" });
    reload();
  };

  if (view.name === "recording") {
    return (
      <MeetingRecording
        projects={projects}
        status={meeting}
        onSaved={(note) => {
          reload();
          openNote(note.id);
        }}
        onDiscarded={backToList}
        onProjectsChanged={reload}
      />
    );
  }
  if (view.name === "note") {
    return (
      <NoteView
        id={view.id}
        projects={projects}
        jobs={jobs}
        onBack={backToList}
        onDeleted={backToList}
        onOpenPostProcess={onOpenPostProcess}
      />
    );
  }
  return (
    <NotesList
      mode={mode}
      notes={notes}
      projects={projects}
      jobs={jobs}
      loaded={loaded}
      onOpen={openNote}
      onRecordMeeting={() => setView({ name: "recording" })}
      onChangeShortcut={onOpenSettings}
      onChanged={reload}
    />
  );
};
