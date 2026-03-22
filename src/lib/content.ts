import type { Message, ContentBlock } from "./types";

export function hasVisibleContent(msg: Message): boolean {
  if (msg.content.length === 0) return false;
  return msg.content.some((block) => {
    switch (block.type) {
      case "text":
      case "thinking":
        return block.text.trim().length > 0;
      case "tool_use":
      case "code_block":
      case "image":
        return true;
      default:
        return false;
    }
  });
}

export function estimateLineCount(content: ContentBlock[]): number {
  let lines = 0;
  for (const block of content) {
    switch (block.type) {
      case "text":
        lines += block.text.split("\n").length;
        break;
      case "code_block":
        lines += block.code.split("\n").length;
        break;
      case "tool_use":
      case "thinking":
        lines += 2;
        break;
      default:
        lines += 1;
    }
  }
  return lines;
}
