import type { ChatMessage } from "../lib/types";

interface UserMessageProps {
  message: ChatMessage;
}

export function UserMessage({ message }: UserMessageProps) {
  return (
    <div className="flex justify-end">
      <div className="max-w-[80%] rounded-2xl rounded-br-sm bg-[#e8e8e8] px-4 py-3 text-text-primary text-[15px] leading-relaxed whitespace-pre-wrap">
        {message.content}
      </div>
    </div>
  );
}
