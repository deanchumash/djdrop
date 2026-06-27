import './App.css';
import { Circle } from './components/Circle';

function App() {
  const handleDrop = (input: string) => {
    console.log('dropped:', input); // wired up in Task 4
  };

  return <Circle onDrop={handleDrop} />;
}

export default App;
