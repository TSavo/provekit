package demo;

import com.google.gson.Gson;
import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Objects;
import org.apache.commons.codec.binary.Base64;
import org.apache.commons.io.IOUtils;
import org.apache.commons.text.StringEscapeUtils;

public final class ToyPipeline {
  private static final Gson GSON = new Gson();

  private ToyPipeline() {}

  public static final class SampleRecord {
    public String name;
    public int count;
    public String[] tags;

    public SampleRecord() {}

    public SampleRecord(String name, int count, String[] tags) {
      this.name = name;
      this.count = count;
      this.tags = tags;
    }

    @Override
    public boolean equals(Object other) {
      if (!(other instanceof SampleRecord)) {
        return false;
      }
      SampleRecord rhs = (SampleRecord) other;
      return Objects.equals(name, rhs.name) && count == rhs.count && Arrays.equals(tags, rhs.tags);
    }

    @Override
    public int hashCode() {
      return Objects.hash(name, count, Arrays.hashCode(tags));
    }
  }

  public static SampleRecord sample() {
    return new SampleRecord("alpha", 7, new String[] {"io", "text"});
  }

  public static String json(SampleRecord record) {
    return GSON.toJson(record);
  }

  public static String standardBase64Json(SampleRecord record) {
    return Base64.encodeBase64String(json(record).getBytes(StandardCharsets.UTF_8));
  }

  public static String ioRoundTrip(String value) throws Exception {
    ByteArrayInputStream in = new ByteArrayInputStream(value.getBytes(StandardCharsets.UTF_8));
    return IOUtils.toString(in, StandardCharsets.UTF_8);
  }

  public static String textRoundTrip(String value) {
    return StringEscapeUtils.unescapeJson(StringEscapeUtils.escapeJson(value));
  }

  public static SampleRecord decodeStandardBase64Json(String encoded) {
    byte[] decoded = Base64.decodeBase64(encoded);
    return GSON.fromJson(new String(decoded, StandardCharsets.UTF_8), SampleRecord.class);
  }

  public static SampleRecord goodRoundTrip(SampleRecord record) throws Exception {
    String encoded = standardBase64Json(record);
    String streamed = ioRoundTrip(encoded);
    String transformed = textRoundTrip(streamed);
    return decodeStandardBase64Json(transformed);
  }

  public static SampleRecord badWiringRoundTripForSample(SampleRecord record) throws Exception {
    String encoded = standardBase64Json(record);
    UrlSafeBase64Sink.requireUrlSafeBase64(encoded);
    String streamed = ioRoundTrip(encoded);
    String transformed = textRoundTrip(streamed);
    return decodeStandardBase64Json(transformed);
  }

  public static final class UrlSafeBase64Sink {
    private UrlSafeBase64Sink() {}

    public static void requireUrlSafeBase64(String encoded) {
      if (encoded.indexOf('+') >= 0 || encoded.indexOf('/') >= 0) {
        throw new IllegalArgumentException("expected URL-safe Base64 alphabet");
      }
    }
  }
}
