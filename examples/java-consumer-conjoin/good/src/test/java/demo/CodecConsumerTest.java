package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.google.gson.Gson;
import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import org.apache.commons.codec.binary.Base64;
import org.apache.commons.io.IOUtils;
import org.apache.commons.text.StringEscapeUtils;
import org.junit.jupiter.api.Test;

final class CodecConsumerTest {
    @Test
    void libraryResultsAgree() throws Exception {
        String json = new Gson().toJson(new Payload("f", 1));
        int gsonLength = json.length();
        String encoded = Base64.encodeBase64String("f".getBytes(StandardCharsets.UTF_8));
        int codecLength = encoded.length();
        String streamed = IOUtils.toString(new ByteArrayInputStream(encoded.getBytes(StandardCharsets.UTF_8)), StandardCharsets.UTF_8);
        int ioLength = streamed.length();
        int textLength = StringEscapeUtils.unescapeJson(StringEscapeUtils.escapeJson(streamed)).length();

        assertEquals(23, gsonLength);
        assertEquals(23, gsonLength);
        assertEquals(4, codecLength);
        assertEquals(4, codecLength);
        assertEquals(4, ioLength);
        assertEquals(4, ioLength);
        assertEquals(4, textLength);
        assertEquals(4, textLength);
    }

    static final class Payload {
        final String value;
        final int count;

        Payload(String value, int count) {
            this.value = value;
            this.count = count;
        }
    }
}
