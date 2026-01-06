import sys
import os
import numpy as np

# Add src to python path
sys.path.append(os.path.join(os.getcwd(), 'src'))

from minna.core.vector_db import VectorManager

def test_vector_logic():
    print("Initializing VectorManager...")
    vm = VectorManager()
    
    text = "Hello, Minna!"
    print(f"Embedding text: '{text}'")
    
    embedding = vm.embed_text(text)
    
    # Check type
    if not isinstance(embedding, list):
        print("FAILED: Embedding is not a list")
        return False
    
    if not all(isinstance(x, float) for x in embedding):
        print("FAILED: Embedding contains non-float values")
        return False
        
    # Check dimensions
    dim = len(embedding)
    print(f"Embedding dimension: {dim}")
    if dim != 512:
        print(f"FAILED: Expected 512 dimensions, got {dim}")
        return False
        
    # Check L2 Normalization
    vec = np.array(embedding)
    norm = np.linalg.norm(vec)
    print(f"L2 Norm: {norm}")
    
    if not np.isclose(norm, 1.0, atol=1e-5):
        print(f"FAILED: Vector is not L2 normalized. Norm={norm}")
        return False
        
    print("SUCCESS: specific vector logic verified.")
    return True

if __name__ == "__main__":
    try:
        if test_vector_logic():
            sys.exit(0)
        else:
            sys.exit(1)
    except Exception as e:
        print(f"An error occurred: {e}")
        sys.exit(1)
