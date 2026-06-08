package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;

import org.junit.jupiter.api.Test;

public final class ToyPipelineGoodTest {
  @Test
  public void roundTripsThroughGsonCodecIoText() throws Exception {
    ToyPipeline.SampleRecord original = ToyPipeline.sample();
    assertEquals(original, ToyPipeline.goodRoundTrip(original));
  }

  @Test
  public void sampleStandardBase64UsesNoUrlUnsafeCharacter() {
    String encoded = ToyPipeline.standardBase64Json(ToyPipeline.sample());
    assertFalse(encoded.contains("+"));
    assertFalse(encoded.contains("/"));
  }
}
