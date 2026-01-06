import os
import sys
import unittest
import shutil
import tempfile
from pathlib import Path

# Add src to python path to import minna modules
sys.path.append(os.path.join(os.getcwd(), 'src'))

from minna.core.vector_db import VectorManager

class Document:
    def __init__(self, content, source="test", metadata=None):
        self.content = content
        self.source = source
        self.metadata = metadata or {}

class TestFiltering(unittest.TestCase):
    def setUp(self):
        # Create a temporary directory for the database
        self.test_dir = tempfile.mkdtemp()
        self.db_path = os.path.join(self.test_dir, "test_minna.db")
        self.vm = VectorManager(db_path=self.db_path)

    def tearDown(self):
        # Clean up the temporary directory
        shutil.rmtree(self.test_dir)

    def test_filtering_search(self):
        docs = [
            Document(content="This is a valid document with sufficient length.", source="valid_source"),
            Document(content="Short", source="valid_source"), # Too short
            Document(content="This is meaningful content but source is unknown", source="Unknown"), # Invalid source
             # Note: VectorManager.add_documents might fail or behave oddly with None content depending on implementation,
             # but we'll try to add one that effectively behaves like empty/bad content if possible, 
             # or rely on the fact that the SQL filter should catch it if it gets in.
             # Python's sqlite3 adapter might complain about None for 'text' fields if not handled, 
             # but let's assume valid string inputs for now that we want to filter OUT.
            Document(content="", source="valid_source"), # Empty string
        ]
        
        # We need to hackily insert a NULL content row if add_documents doesn't support it directly,
        # or just test the other conditions. The requirement says "content IS NOT NULL".
        # Let's try adding these first.
        self.vm.add_documents(docs)
        
        # Manually insert a NULL content row to test that specifically
        conn = self.vm._get_connection()
        c = conn.cursor()
        # id, source, content, metadata
        c.execute("INSERT INTO documents (id, source, content, metadata) VALUES (?, ?, ?, ?)", 
                  ("null_id", "valid_source", None, "{}"))
        # We also need a vector for it if we want it to be considered in vector search (INNER JOIN likely),
        # but if the query joins on rowid and the vector exists, it might show up.
        # Actually, if content is NULL, embed_text might fail during normal addition.
        # But if it exists in DB (legacy data), we want to filter it.
        # Let's add a dummy vector for it.
        c.execute("SELECT rowid FROM documents WHERE id = 'null_id'")
        row_id = c.fetchone()[0]
        dummy_embedding = [0.0] * 512
        # We need to import json to dump embedding
        import json
        c.execute("INSERT INTO vec_documents (rowid, embedding) VALUES (?, ?)", (row_id, json.dumps(dummy_embedding)))
        conn.commit()
        conn.close()

        # Test Vector Search
        # We search for "content" which should match the valid doc.
        results_dict = self.vm.search("content")
        results = results_dict["results"]
        
        print("\nVector Search Results:", results)

        found_valid = False
        for res in results:
            self.assertNotEqual(res['content'], "Short")
            self.assertNotEqual(res['content'], "")
            self.assertIsNotNone(res['content'])
            self.assertNotEqual(res['source'], "Unknown")
            
            if res['content'] == "This is a valid document with sufficient length.":
                found_valid = True
        
        self.assertTrue(found_valid, "Should find the valid document")
        
    def test_filtering_keyword(self):
         docs = [
            Document(content="Keyword match valid document length", source="valid_source"),
            Document(content="Key", source="valid_source"), # Too short match
            Document(content="Keyword match but unknown source", source="Unknown"), # Invalid source
        ]
         self.vm.add_documents(docs)
         
         # Keyword Search
         results = self.vm.search_keyword("Key")
         print("\nKeyword Search Results:", results)
         
         found_valid = False
         for res in results:
             self.assertNotEqual(res['content'], "Key")
             self.assertNotEqual(res['source'], "Unknown")
             if res['content'] == "Keyword match valid document length":
                 found_valid = True
         
         self.assertTrue(found_valid, "Should find the valid document in keyword search")

if __name__ == '__main__':
    unittest.main()
