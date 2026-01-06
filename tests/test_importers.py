import unittest
import json
from minna.ingestion.importers import OpenAIImporter, AnthropicImporter, GoogleTakeoutImporter

class TestImporters(unittest.TestCase):
    def test_openai_importer(self):
        sample_data = [{
            "id": "conv-1",
            "title": "OpenAI Test",
            "mapping": {
                "node-1": {
                    "message": {
                        "author": {"role": "user"},
                        "content": {"parts": ["Hello AI"]},
                        "create_time": 1704067200 # 2024-01-01 00:00:00
                    }
                },
                "node-2": {
                    "message": {
                        "author": {"role": "assistant"},
                        "content": {"parts": ["Hello Human"]},
                        "create_time": 1704067201
                    }
                }
            }
        }]
        importer = OpenAIImporter()
        docs = importer.import_conversations(sample_data)
        self.assertEqual(len(docs), 2)
        self.assertEqual(docs[0].content, "Hello AI")
        self.assertEqual(docs[0].metadata["role"], "user")
        self.assertEqual(docs[1].content, "Hello Human")
        self.assertEqual(docs[1].metadata["role"], "assistant")

    def test_anthropic_importer(self):
        sample_data = [{
            "uuid": "claude-1",
            "name": "Claude Test",
            "chat_messages": [
                {"sender": "human", "text": "What is 2+2?", "created_at": "2024-01-01T00:00:00Z"},
                {"sender": "assistant", "text": "4", "created_at": "2024-01-01T00:00:01Z"}
            ]
        }]
        importer = AnthropicImporter()
        docs = importer.import_conversations(sample_data)
        self.assertEqual(len(docs), 2)
        self.assertEqual(docs[0].content, "What is 2+2?")
        self.assertEqual(docs[0].metadata["role"], "user")
        self.assertEqual(docs[1].content, "4")
        self.assertEqual(docs[1].metadata["role"], "assistant")

    def test_google_takeout_importer_html(self):
        html_content = """
        <html>
            <body>
                <div>Prompt</div>
                <div>Who are you?</div>
                <div>Response</div>
                <div>I am Gemini.</div>
            </body>
        </html>
        """
        importer = GoogleTakeoutImporter()
        docs = importer.import_conversations(html_content)
        self.assertEqual(len(docs), 2)
        self.assertEqual(docs[0].content, "Who are you?")
        self.assertEqual(docs[0].metadata["role"], "user")
        self.assertEqual(docs[1].content, "I am Gemini.")
        self.assertEqual(docs[1].metadata["role"], "assistant")

    def test_google_takeout_importer_json(self):
        sample_data = [
            {
                "prompt": {"text": "Explain quantum physics"},
                "candidates": [{"text": "It is complex."}]
            }
        ]
        importer = GoogleTakeoutImporter()
        docs = importer.import_conversations(json.dumps(sample_data))
        self.assertEqual(len(docs), 2)
        self.assertEqual(docs[0].content, "Explain quantum physics")
        self.assertEqual(docs[1].content, "It is complex.")

if __name__ == '__main__':
    unittest.main()
