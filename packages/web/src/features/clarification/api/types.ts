export type ClarificationType = 'announcement' | 'question' | 'direct_message';

export interface Clarification {
  id: number;
  contest_id: number;
  author_id: number;
  author_name: string;
  content: string;
  clarification_type: ClarificationType;
  recipient_id: number | null;
  recipient_name: string | null;
  is_public: boolean;
  reply_content: string | null;
  reply_author_id: number | null;
  reply_author_name: string | null;
  reply_is_public: boolean;
  replied_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ClarificationListResponse {
  data: Clarification[];
}

export interface CreateClarificationBody {
  content: string;
  clarification_type: ClarificationType;
  recipient_id?: number;
  is_public?: boolean;
}

export interface ReplyClarificationBody {
  content: string;
  is_public: boolean;
}
