-- Add agent attribution to messages table
-- Allows tracking which agent (conversational, knowledge, etc.) produced each message

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'messages') THEN
    -- Add agent column if it doesn't exist
    IF NOT EXISTS (
      SELECT 1 FROM information_schema.columns 
      WHERE table_name = 'messages' AND column_name = 'agent'
    ) THEN
      ALTER TABLE messages
      ADD COLUMN agent VARCHAR(50) DEFAULT 'assistant';
    END IF;

    -- Create index for efficient agent-based queries
    CREATE INDEX IF NOT EXISTS idx_messages_agent ON messages(agent);
  END IF;
END $$;
