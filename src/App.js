import { useEffect, useState } from "react";
import {
  ChakraProvider,
  Center,
  Heading,
  Box,
  Stack,
  Button,
  SimpleGrid,
} from "@chakra-ui/react";
import * as tauri from "tauri/api/tauri";

import IAlert from "./components/Alert";

import SolveSettings from "./pages/SolveSettings";
import BasicSettings from "./pages/BasicSettings";

function App() {
  const [appState, setAppState] = useState(0);
  const [errMsg, setErrMsg] = useState("");
  const [config, setConfig] = useState("");

  useEffect(() => {
    if (appState > 0) {
      tauri.promisified({ cmd: "tryDropVideo" });
    }
  }, [appState]);

  return (
    <ChakraProvider>
      <Box h="800px" bg="#282828">
        <IAlert errMsg={errMsg} onClose={() => setErrMsg("")} />
        {errMsg === "" &&
          <SimpleGrid columns={2}>
            <Button
              rounded={false}
              bg="#98971a"
              color="#32302f"
              onClick={() => setAppState(0)}
            >
              基础配置
            </Button>
            <Button
              rounded={false}
              bg="#458588"
              color="#32302f"
              onClick={() => {
                if (config.thermocouples.length === 0) {
                  setErrMsg("未设置热电偶");
                  return;
                }
                setAppState(1);
              }}
            >
              求解设置
            </Button>
          </SimpleGrid>
        }
        <Center>
          <Heading
            color="#689d6a"
            marginBottom="5px"
            fontSize="3xl"
          >
            当前实验组：{config.case_name}
          </Heading>
        </Center>
        <Stack>
          {appState === 0 &&
            <BasicSettings
              config={config}
              setConfig={setConfig}
              setErrMsg={setErrMsg}
            />}
          {appState === 1 &&
            <SolveSettings
              config={config}
              setConfig={setConfig}
              setErrMsg={setErrMsg}
            />}
        </Stack>
      </Box>
    </ChakraProvider >
  )
}

export default App;
