import unittest
from unittest.mock import MagicMock, patch
from minna.ingestion.slack import SlackConnector
from minna.core.vector_db import VectorManager
from minna.ingestion.base import Document

class TestParserAndFilter(unittest.TestCase):
    def test_extract_text_cleanup(self):
        connector = SlackConnector(slack_token="fake")
        
        # Test 1: Plain text
        msg1 = {"text": "Hello World"}
        self.assertEqual(connector._extract_text_from_message(msg1), "Hello World")
        
        # Test 2: User tag cleanup
        msg2 = {"text": "Hello <@U12345> World"}
        # Expect "Hello  World" (double space) or similar. My regex was <@U[A-Z0-9]+> replace with empty string.
        self.assertEqual(connector._extract_text_from_message(msg2), "Hello  World")
        
        # Test 3: Fallback from blocks (simulated by having text present)
        # We didn't implement block parsing yet, just fallback.
        msg3 = {"blocks": [...], "text": "Fallback Text"}
        self.assertEqual(connector._extract_text_from_message(msg3), "Fallback Text")
        
    @patch('builtins.print')
    @patch('minna.core.vector_db.VectorManager.embed_text')
    @patch('minna.core.vector_db.VectorManager._get_connection')
    def test_drop_logging(self, mock_conn, mock_embed, mock_print):
        vm = VectorManager(db_path=":memory:")
        mock_embed.return_value = [0.1, 0.2, 0.3] # Serializable return value
        
        # Doc 1: Too short
        doc1 = Document(source="slack", content="Hi", metadata={"channel_name": "general"})
        # Doc 2: Unknown source
        doc2 = Document(source="Unknown", content="Long enough message", metadata={})
        # Doc 3: Good
        doc3 = Document(source="slack", content="This is a valid long message.", metadata={})
        
        vm.add_documents([doc1, doc2, doc3])
        
        # Verify print calls
        # We expect 2 warning prints
        # print(f"⚠️ Dropped message from #{channel}: '{clean_content[:20]}...' (Reason: Too short/Empty)")
        # print(f"⚠️ Dropped message. Reason: Unknown source.")
        
        # Check that we printed the drop messages
        printed_messages = [call[0][0] for call in mock_print.call_args_list]
        
        self.assertTrue(any("Reason: Too short/Empty" in m for m in printed_messages))
        self.assertTrue(any("Reason: Unknown source" in m for m in printed_messages))
        self.assertTrue(any("Successfully added 1 documents" in m for m in printed_messages))

if __name__ == '__main__':
    unittest.main()
