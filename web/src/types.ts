export interface SaveData {
  id: string;
  sequence: number;
  message: string | null;
  file_count: number;
  root_hash: string;
  created_at: string;
  diff_added: number;
  diff_modified: number;
  diff_removed: number;
}

export interface CheckpointData extends Record<string, unknown> {
  id: string;
  title: string;
  prompt: string | null;
  response: string | null;
  agent: string | null;
  session_id: string | null;
  parent_save_id: string | null;
  created_at: string;
  private: boolean;
  is_head: boolean;
  is_main_chain: boolean;
  saves: SaveData[];
}

export interface GraphData {
  checkpoints: CheckpointData[];
  head_save_id: string | null;
  project_name: string;
}
