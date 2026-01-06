import json
import os
from typing import List, Dict, Optional, Any
from datetime import datetime
from bs4 import BeautifulSoup
from .base import Document

# Alias for compatibility with user request
MinnaDocument = Document

class OpenAIImporter:
    """
    Parses OpenAI (ChatGPT) conversations.json export.
    """
    def import_conversations(self, data: List[Dict[str, Any]]) -> List[MinnaDocument]:
        documents = []
        for conv in data:
            conv_id = conv.get("id", "unknown")
            title = conv.get("title", "Untitled Conversation")
            mapping = conv.get("mapping", {})
            
            for node_id, node in mapping.items():
                message = node.get("message")
                if not message:
                    continue
                
                author = message.get("author", {})
                role = author.get("role")
                if role not in ["user", "assistant"]:
                    continue
                
                content = message.get("content", {})
                parts = content.get("parts", [])
                
                # Join parts into a single string
                text = ""
                for part in parts:
                    if isinstance(part, str):
                        text += part
                    elif isinstance(part, dict):
                        # Sometimes parts can be rich content, we just take the 'text' or skip
                        text += part.get("text", "")
                
                if not text:
                    continue
                
                create_time = message.get("create_time")
                timestamp = datetime.fromtimestamp(create_time).isoformat() if create_time else None
                
                metadata = {
                    "conversation_id": conv_id,
                    "conversation_title": title,
                    "role": role,
                    "timestamp": timestamp,
                    "source_format": "openai"
                }
                
                documents.append(MinnaDocument(
                    source="openai",
                    content=text,
                    metadata=metadata
                ))
        return documents

class AnthropicImporter:
    """
    Parses Anthropic (Claude) conversations.json export.
    """
    def import_conversations(self, data: List[Dict[str, Any]]) -> List[MinnaDocument]:
        documents = []
        for conv in data:
            conv_id = conv.get("uuid", "unknown")
            name = conv.get("name", "Untitled")
            chat_messages = conv.get("chat_messages", [])
            
            for msg in chat_messages:
                sender = msg.get("sender")
                if sender not in ["human", "assistant"]:
                    continue
                
                text = msg.get("text", "")
                if not text:
                    continue
                
                create_time = msg.get("created_at") # Claude uses ISO strings usually
                
                metadata = {
                    "conversation_id": conv_id,
                    "conversation_name": name,
                    "role": "user" if sender == "human" else "assistant",
                    "timestamp": create_time,
                    "source_format": "anthropic"
                }
                
                documents.append(MinnaDocument(
                    source="anthropic",
                    content=text,
                    metadata=metadata
                ))
        return documents

class GoogleTakeoutImporter:
    """
    Parses Gemini exports (HTML or JSON) from Google Takeout.
    """
    def import_from_html(self, html_content: str) -> List[MinnaDocument]:
        soup = BeautifulSoup(html_content, "html.parser")
        documents = []
        
        # Takeout Gemini HTML usually has blocks like "Prompt" and "Response"
        # We'll look for text containing these labels or specific classes if they exist.
        # Since the structure is "messy", we'll try to find div/p elements that look like messages.
        
        # Find all divs that might contain messages
        potential_blocks = soup.find_all(["div", "p"])
        
        current_role = None
        current_text = []
        
        for block in potential_blocks:
            text = block.get_text(strip=True)
            if not text:
                continue
                
            if "Prompt" in text and len(text) < 20: # Likely a header
                if current_role and current_text:
                    documents.append(self._create_doc(current_role, "\n".join(current_text)))
                current_role = "user"
                current_text = []
            elif "Response" in text and len(text) < 20: # Likely a header
                if current_role and current_text:
                    documents.append(self._create_doc(current_role, "\n".join(current_text)))
                current_role = "assistant"
                current_text = []
            else:
                if current_role:
                    current_text.append(text)
        
        # Add last one
        if current_role and current_text:
            documents.append(self._create_doc(current_role, "\n".join(current_text)))
            
        return documents

    def _create_doc(self, role: str, content: str) -> MinnaDocument:
        return MinnaDocument(
            source="google_takeout",
            content=content,
            metadata={
                "role": role,
                "source_format": "google_takeout_html"
            }
        )

    def import_conversations(self, file_path_or_content: str) -> List[MinnaDocument]:
        # If it's a file path, read it; otherwise assume it's content
        if os.path.exists(file_path_or_content):
            with open(file_path_or_content, "r", encoding="utf-8") as f:
                content = f.read()
        else:
            content = file_path_or_content
            
        if content.strip().startswith("<!DOCTYPE html") or "<html" in content.lower():
            return self.import_from_html(content)
        
        # If it's JSON, parse it (Gemini also provides JSON in some exports)
        try:
            data = json.loads(content)
            return self._import_from_json(data)
        except json.JSONDecodeError:
            # Fallback to HTML parsing if it looks like HTML but missing tags
            return self.import_from_html(content)

    def _import_from_json(self, data: Any) -> List[MinnaDocument]:
        # Handle JSON structure if Gemini provides one
        documents = []
        # Typically list of objects with 'prompt' and 'candidates'
        if isinstance(data, list):
            for item in data:
                prompt = item.get("prompt", {}).get("text")
                if prompt:
                    documents.append(self._create_doc("user", prompt))
                
                candidates = item.get("candidates", [])
                for cand in candidates:
                    resp = cand.get("text")
                    if resp:
                        documents.append(self._create_doc("assistant", resp))
        return documents
