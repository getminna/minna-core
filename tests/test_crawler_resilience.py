import unittest
from unittest.mock import MagicMock, patch
from minna.ingestion.slack import SlackConnector, Document
from slack_sdk.errors import SlackApiError

class TestCrawlerResilience(unittest.TestCase):
    @patch('minna.ingestion.slack.WebClient')
    @patch('time.sleep') # Skip sleeps
    def test_double_sweep(self, mock_sleep, mock_web_client):
        # Setup Mock
        client_instance = mock_web_client.return_value
        connector = SlackConnector(slack_token="fake")
        
        # 1. Mock Channels
        # Channel 1: Good
        # Channel 2: Fails once (Sweep 1), Succeeds later? 
        # Actually Double Sweep logic: Sweep 1 fails -> Add to Queue -> Sweep 2 Retry.
        # So we need the mock to fail on first call for Channel 2, succeed on second.
        
        c1 = {"id": "C1", "name_normalized": "good-chan", "is_archived": False}
        c2 = {"id": "C2", "name_normalized": "flaky-chan", "is_archived": False}
        c3 = {"id": "C3", "name_normalized": "bad-chan", "is_archived": False}
        c4 = {"id": "C4", "name_normalized": "archived-chan", "is_archived": True}
        
        # Mock _fetch_channels to return these
        connector._fetch_channels = MagicMock(return_value=[c1, c2, c3, c4])
        
        # Mock _process_channel to simulate behavior
        # We need a side_effect that checks which channel is being processed
        # and maintains state for the "flaky" one.
        
        call_counts = {"C1": 0, "C2": 0, "C3": 0}
        
        def side_effect(channel, since_ts):
            cid = channel["id"]
            if cid == "C4":
                # Should not be called if filtered correctly
                raise Exception("Archived channel was processed!")
                
            call_counts[cid] = call_counts.get(cid, 0) + 1
            
            if cid == "C1":
                return [Document(source="slack", content="doc1", metadata={})]
            elif cid == "C2":
                if call_counts[cid] == 1:
                    raise Exception("Random Network Error")
                return [Document(source="slack", content="doc2", metadata={})]
            elif cid == "C3":
                # Raise a specific SlackApiError to test readable conversion
                raise SlackApiError(
                    "Missing Scope", 
                    {"ok": False, "error": "missing_scope"}
                )
            return []

        connector._process_channel = MagicMock(side_effect=side_effect)
        
        # Run Sync
        print("\n--- Starting Test Sync ---")
        docs = connector.sync(since_timestamp=123.0)
        print("--- End Test Sync ---\n")
        
        # Verifications
        # C1: Success in Sweep 1. Called 1 time.
        self.assertEqual(call_counts["C1"], 1)
        
        # C2: Fail in Sweep 1, Success in Sweep 2. Called 2 times.
        self.assertEqual(call_counts["C2"], 2)
        
        # C3: Fail in Sweep 1, Fail in Sweep 2. Called 2 times.
        self.assertEqual(call_counts["C3"], 2)
        
        # C4: Never called (Archived).
        # We didn't track C4 in call_counts but if it was called it would raise Exception.
        
        # Docs should contain C1 and C2 docs (C3 failed).
        self.assertEqual(len(docs), 2)
        print("Test Passed!")

if __name__ == '__main__':
    unittest.main()
