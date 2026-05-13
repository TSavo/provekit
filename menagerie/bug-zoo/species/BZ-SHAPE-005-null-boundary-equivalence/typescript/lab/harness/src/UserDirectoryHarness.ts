import { lookup } from "../../library/src/UserDirectory";

const result = lookup("ada");
if (result !== "user:ADA") {
  throw new Error(`unexpected lookup result: ${result}`);
}
