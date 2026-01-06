from abc import ABC, abstractmethod
from typing import List, Dict, Optional
from pydantic import BaseModel, Field

class Document(BaseModel):
    """
    Represents a unified document for ingestion.
    """
    source: str
    content: str
    metadata: Dict = Field(default_factory=dict)
    
class BaseConnector(ABC):
    """
    Abstract base class for all ingestion connectors.
    """
    
    @abstractmethod
    def sync(self, since_timestamp: float) -> List[Document]:
        """
        Fetches data from the source since the given timestamp.
        """
        pass
