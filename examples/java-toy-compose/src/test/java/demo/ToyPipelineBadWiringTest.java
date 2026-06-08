package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

public final class ToyPipelineBadWiringTest {
  @Test
  public void sampledStandardBase64CanEnterUrlSafeSink() throws Exception {
    ToyPipeline.SampleRecord original = ToyPipeline.sample();
    assertEquals(original, ToyPipeline.badWiringRoundTripForSample(original));
  }
}
